# octa

**チーム開発グレードの協働を、個人の AI エージェント駆動開発にローカルでもたらす CLI。**

octa は、1台のマシンで複数の AI エージェントと複数の開発セッションを並行して動かす個人開発者のための、ローカル協働基盤です。

Issue、Pull Request、Wiki という GitHub 風のメンタルモデルで、リポジトリが何を目指し、何が残っているかを、セッションをまたいで残します。

データは外部サービスに送信せず、ローカルの SQLite に保存されます。

## octa が解決すること

AI エージェントに作業を分けると、判断の背景、未処理の作業、次の担当者へ渡すべき文脈がセッションごとに散ります。

octa は、その情報をリポジトリ単位で持続する記録にします。

- **Issue**：状態、依存関係、コメント、ラベル、原子的な lease を持つ作業記録です。
- **Pull Request**：Git ブランチに紐づく議論と状態の記録です。コードと diff は Git 側に残ります。
- **Wiki**：方針や手順を残すページです。`[[slug]]` によるリンクと backlink を使えます。
- **Label と state**：各プロジェクトの分類と作業フローを設定できます。

octa は Git hosting、Web UI、リモート同期、認証、多人数のリアルタイム協働を提供しません。

同じマシン上で動く複数の worktree、エージェント、セッションの調整に焦点を絞っています。

## 前提条件

- Git リポジトリの中で実行すること。
- Rust 1.89 以降と Cargo を使えること。

このリポジトリには Nix の開発環境もあります。

```sh
nix develop
cargo build
```

ローカルにインストールして `octa` コマンドとして使うには、次を実行します。

```sh
cargo install --path .
```

`~/.cargo/bin` が `PATH` に含まれている必要があります。

インストールせず、開発中のバイナリを使う場合は次を実行します。

```sh
./target/debug/octa --help
```

以降の例では、`octa` が `PATH` に入っているものとします。

## 最初の5分

まず、対象の Git リポジトリに移動します。

```sh
cd path/to/your-repository
```

Issue を作成し、一覧と詳細を確認します。

```sh
octa issue create \
  --title "リリース手順を文書化する" \
  --body "必要な確認項目と実行手順を Wiki に残す。"

octa issue list
octa issue show 1
```

作業を始めるエージェントまたはセッションは、期限のない排他的な **lease** を取得できます。
`issue lock` は `amber-otter-lantern` のような、人間が扱いやすい3単語の lease IDを
標準出力へ一度だけ返します。lease IDはセキュリティcredentialではなく、同時作業の
誤操作を防ぐための所有権IDです。後続のコマンドで再利用できるよう保持します。

```sh
LEASE=$(octa issue lock 1)
octa issue set-state 1 "In Progress" --lease "$LEASE"
octa issue comment 1 --body "着手しました。"
```

完了時は同じ lease を付けて状態を更新し、その後で lease を解放します。

```sh
octa issue set-state 1 Done --lease "$LEASE"
octa issue unlock 1 --lease "$LEASE"
```

lease ID を失った場合は、復旧操作として `--force` で解放できます。
以前の lease ID は即座に無効になり、作業を再開するには新しい lease の取得が必要です。

```sh
octa issue unlock 1 --force
LEASE=$(octa issue lock 1)
```

`issue set-state`、`set`、`unset`、`add`、`remove` と通常の `unlock`、および Issue と PR の link を変更する `pr create --issue`、`pr add`、`pr remove` には、対象 Issue の `--lease` が必要です。
Issue の作成とコメント、PR のコメント、Issue と link しない PR の作成、PR 自体の `set` / `set-state`、Project、Milestone、Wiki、config の操作には lease は不要です。
読み取り操作にも不要です。
ツールログやコマンド引数にlease IDが現れることは想定内です。一方、Issueコメント、
Git成果物、リポジトリファイルなどの永続的な記録には含めません。`issue list` と
`issue show` はlease IDを表示せず、取得中かどうかだけを `leased` で示します。

## Issue で作業を調整する

Issue は番号、本文、コメント、状態、依存関係、ラベルを持ちます。

以下で既存の Issue 1 を変更する例では、先に取得した lease を使います。

```sh
LEASE=$(octa issue lock 1)
```

### 状態と一覧

新規リポジトリには `Backlog`、`Todo`、`In Progress`、`In Review`、`Done`、
`Canceled` の6状態が作られ、新規Issueは `Backlog` に入ります。

```sh
octa issue set-state 1 "In Progress" --lease "$LEASE"
octa issue list --open
octa issue list --closed
octa issue list --all
octa issue list --state "In Progress"
```

引数なしと `--open` はopenなIssue、`--closed` はclosedなIssue、`--all` は
両方を表示します。Issueがclosedかどうかは、その状態にcloseフラグが立っているかで
決まります。`--state <name>` は設定済みの状態名との完全一致です。
これら4つのselectorは同時に指定できません。

Linear の Issue 一覧と詳細の代わりに、read-only の2ペインTUIも使えます。
これは現行の初期実装を説明するものであり、将来のTUI更新操作を製品境界から除外するものではありません。
既定の `filter: all` は、作業候補だけでなく In Review、Done、Canceled、
および既存の custom/legacy state を含む current repository の全Issueを
Issue番号順に表示します。

```sh
octa issue tui
```

`j/k` または矢印でIssueを選択し、`Tab` で一覧と詳細のfocusを切り替えます。
詳細は `PgUp/PgDn` でもscrollでき、`q` または `Esc` で終了します。
この画面からIssueや関連データを変更する操作はありません。

### Project と Milestone

有限の成果を Project としてまとめ、段階が必要な Project には順序付きの
Milestone を作れます。Project と Milestone は名前または番号で参照できます。
`project list` は既定でclosedな Project も含む全 Project を返し、各 tally も
closedな Issue を含む全 Issue を open / closed で数えます。作業中の Project
だけが必要な場合は `project list --active` と明示します。Project がclosedかどうかは
`project create --closed` と `project set-state <project> <state> --closed`
で設定します。Project は作成順に表示されます。優先度が必要な場合は、
`single` のラベルグループを自分で定義してください。

```sh
octa project create --name "CLI を公開する"
octa project list
octa project list --active

octa milestone create --project "CLI を公開する" \
  --name "Public beta" \
  --description "利用者向けbetaを公開する段階" \
  --status active \
  --position 1 \
  --target-date 2026-09-01

octa milestone list --project "CLI を公開する"
octa milestone show "Public beta" --project "CLI を公開する"
octa milestone set "Public beta" --project "CLI を公開する" \
  --status completed \
  --target-date 2026-09-15
```

Issue 作成時に Project と Milestone を同時に指定できます。Milestone は
Project 内の entity なので、`--milestone` には `--project` も必要です。

```sh
octa issue create \
  --title "beta 利用者を招待する" \
  --project "CLI を公開する" \
  --milestone "Public beta"
```

既存 Issue への Milestone の設定・解除と、同じ Milestone に属する Issue の
一覧取得もできます。Project を変更または解除する場合は、先に Milestone を
unset します。

```sh
octa issue set 1 --milestone "Public beta" --lease "$LEASE"
octa issue list --project "CLI を公開する" --milestone "Public beta"
octa issue unset 1 --milestone --lease "$LEASE"
```

Issue の親子関係は同じリポジトリ内で設定でき、Project の所属とは独立しています。
親子は異なる Project に所属でき、片方だけが Project に所属していても構いません。
Project のない既存 Issue に親を設定した時は、その時点の親の Project を初期値として
継承しますが、その後は親子それぞれの Project を変更または解除できます。

```sh
LEASE_2=$(octa issue lock 2)
octa issue set 2 --parent 1 --lease "$LEASE_2"
octa issue set 2 --project "別の Project" --lease "$LEASE_2"
octa issue unset 1 --project --lease "$LEASE"
```

Project 内の Milestone が設定されている Issue だけは、従来どおり先に Milestone を
unset してから Project を変更または解除します。

新規リポジトリには、キャプチャから実行・レビュー・2つの終端までを覆う6状態が
自動作成されます。

| 状態 | フラグ | 備考 |
|---|---|---|
| Backlog | starting | 新規Issueの入口 |
| Todo | | |
| In Progress | | |
| In Review | | |
| Done | closed | Issueはここでcloseする |
| Canceled | closed | Issueはここでcloseする |

seedが走るのは状態を1つも持たないリポジトリだけです。すでにworkflowを
設定済みのリポジトリの状態構成は、そのまま保たれます。

状態はあとから追加・変更・削除できます。

```sh
octa config state create blocked
octa config state set Todo --name Ready
octa config state set Ready --closed true
octa config state delete blocked --move-to Ready
octa config state set-default Ready
octa config state list
```

`config state set --name` での改名は、その状態のIssueも一緒に移します。
`config state delete` は、Issueが残っている状態には `--move-to <state>` を要求し、
入口の状態は `set-default` で入口を移すまで削除できません。

新規Issueが入る状態は1リポジトリにつき1つだけで、`config state set-default`
（または `config state create --starting`）で移します。この明示的なフラグだけで
決まります。

状態は並び順を持ちません。`config state list` の表示順は `is_starting` と
`is_closed` の2フラグと名前から導かれ、入口の状態、残りのopenな状態、closedな状態の
順に並びます。各グループの中は名前順です。

Issueの状態は `closed` フラグひとつでopen/closedが決まります。`issue list --closed`
や `project list` の `Open/Closed` 列が数えているのは、このフラグが立った状態にある
Issueです。

octaが状態について持つ分類はこの2フラグだけで、その間の段階を区別しません。
状態名そのものには何の意味も与えないため、任意の状態名を使えます。
旧バージョンで作成済みのworkflow状態やその他のcustom/legacy stateと、
それらを参照するIssueはmigration後も削除・改名されません。

Issueの状態は `issue set-state` で明示的に遷移させます。

### 依存関係

Issue 1 が Issue 2 をブロックする関係を作るには、次を実行します。

```sh
octa issue add 1 --blocks 2 --lease "$LEASE"
octa issue show 1
octa issue show 2
```

まだ終端状態ではない blocker を持たない作業は、`--unblocked` で一覧できます。

```sh
octa issue list --unblocked
```

依存を削除するには同じプロパティを `remove` します。

```sh
octa issue remove 1 --blocks 2 --lease "$LEASE"
```

順序を持たない関連 Issue は `--related` で結びます。同じ組を逆順で追加しても一件だけ保存され、`--related-to` で候補を絞れます。

```sh
octa issue add 1 --related 2 --lease "$LEASE"
octa issue list --related-to 1
octa issue remove 2 --related 1 --lease "$LEASE_2"
```

### ラベル

ラベルは単独でも使えます。
ラベル名とグループ名はリポジトリごとに自由に決められ、octa が予約する分類名や
`impl` / `design` / `research` のような特別扱いされるラベルはありません。

```sh
octa config label create documentation --target issue
octa issue add 1 --label documentation --lease "$LEASE"
octa issue remove 1 --label documentation --lease "$LEASE"
```

`single` グループでは、同じグループのラベルを一つだけ付けられます。

`multi` グループでは、同じグループのラベルを複数共存させられます。

```sh
octa config label-group create priority --target issue --selection single
octa config label create high --target issue --group priority
octa config label create low --target issue --group priority
octa issue add 1 --label high --lease "$LEASE"

octa config label-group create area --target issue --selection multi
octa config label create cli --target issue --group area
octa config label create storage --target issue --group area
octa issue add 1 --label cli --lease "$LEASE"
octa issue add 1 --label storage --lease "$LEASE"
```

Project用のラベル定義はIssue用とは分かれています。同じ名前も別々に定義でき、
`--target`は必須です。

```sh
octa config label-group create horizon --target project --selection single
octa config label create now --target project --group horizon
octa config label create next --target project --group horizon
octa project add "CLI を公開する" --label now
octa project remove "CLI を公開する" --label now
```

## Pull Request の議論を残す

octa の Pull Request は、ブランチに紐づく番号付きの議論エンティティです。

コードと diff は Git が扱い、octa は状態とコメントを保持します。

```sh
octa pr create \
  --title "リリース手順を追加する" \
  --branch docs/release-process \
  --body "Wiki と README を更新する。" \
  --issue 1 \
  --lease "$LEASE"

octa pr comment 1 --body "確認をお願いします。"
octa pr show 1
octa pr set-state 1 closed
```

既存 PR は作成時と同じ形のまま利用でき、必要になった時だけ Issue と明示的に link できます。
1つの Issue に複数の PR、1つの PR に複数の Issue を link できます。同じ組は重複保存されません。

```sh
octa pr add 2 --issue 1 --lease "$LEASE"
octa issue show 1
octa pr remove 2 --issue 1 --lease "$LEASE"
```

PR 一覧は `open`、`closed`、`all` で絞り込めます。

```sh
octa pr list --state open
octa pr list --state all
```

## Wiki に方針と手順を残す

Wiki はリポジトリ内のファイルではなく、octa のローカルストアに保存されます。

slug を省略すると、タイトルから ASCII 英数字とハイフンの slug を自動生成します。

日本語だけのタイトルなど、自動生成後に slug が空になるタイトルでは `--slug` を指定してください。

```sh
octa wiki create \
  --title "リリース手順" \
  --slug release-process \
  --body "関連する方針は [[development-policy]] を参照する。"

octa wiki show release-process
octa wiki list
```

明示的な slug を指定することもできます。

```sh
octa wiki create \
  --title "開発方針" \
  --slug development-policy \
  --body "設計判断をここに残す。"
```

本文中の `[[slug]]` はリンクとして記録されます。

`wiki show` は、そのページからのリンクと、そのページへの backlink を表示します。

## GraphQL で必要なデータだけ読む

`octa query` は、現在のリポジトリを既定 scope とする read-only GraphQL schema を提供します。
document は標準入力か `--file` から渡し、variables は JSON object で指定します。

```sh
octa query --variables '{"number": 25}' <<'GRAPHQL'
query IssueContext($number: Int!) {
  issue(number: $number) {
    number
    title
    leased
    project { name }
    labels { name }
    blocks(limit: 20) { number title }
  }
}
GRAPHQL

octa query --file query.graphql --variables '{"limit": 20}'
```

selection setは必要な列とrelationだけを取得するSQLite queryへ変換されます。
単一relationは相関JOIN、複数relationはJOINを含む集約subqueryになり、選択されて
いないrelationへはアクセスしません。応答はGraphQL JSON envelopeで、実行した
query数を`extensions.dbAccesses`に含めます。list fieldの`limit`は既定50・最大100、
query depthは8、complexityは500が上限です。schemaにmutationはありません。
Issueの`leased` fieldは取得中かどうかだけを返し、lease IDは公開しません。

成功時も検証エラー時も標準GraphQL JSON envelopeを返します。
成功時は `data`、検証エラー時は `errors` が含まれるため、CLIの終了statusだけでなくenvelopeを確認します。

```json
{"data":{"issue":{"number":25,"title":"read-only GraphQL query surfaceを追加する"}},"extensions":{"dbAccesses":1}}
```

```sh
printf '%s\n' '{ missingField }' | octa query
```

```json
{"data":null,"extensions":{"dbAccesses":0},"errors":[{"message":"Unknown field \"missingField\" on type \"QueryRoot\".","locations":[{"line":1,"column":3}]}]}
```

利用可能な型とfieldはintrospection、またはSDL出力で確認できます。

```sh
octa query --schema
```

## JSON 出力

自動化やエージェントから利用する場合は、対応するコマンドに `--json` を付けます。

```sh
octa issue create --title "調査する" --json
octa issue list --all --json
octa issue show 1 --json
octa pr list --state all --json
octa wiki show release-process --json
octa config label list --target issue --json
```

## worktree とリポジトリのスコープ

通常は、現在いる Git リポジトリが対象です。

Git の common directory を識別子に使うため、同じリポジトリの複数 worktree は同じ octa データを共有します。

別の登録済みリポジトリを明示するには `--repo` を使います。

```sh
octa --repo other-repository issue list
```

読み取り系の一部の一覧では、`--all-repos` で登録済みリポジトリを横断できます。

```sh
octa --all-repos issue list --all
octa --all-repos pr list --state all
octa --all-repos wiki list
```

更新操作と、Issue の `--label`、`--unblocked`、固有状態による絞り込みは、単一リポジトリで実行してください。

## 保存場所とバックアップ

octa は、ユーザーごとに一つの SQLite データベースを使います。

```text
$XDG_DATA_HOME/octa/octa.db
```

`XDG_DATA_HOME` が未設定の場合は、次の場所です。

```text
~/.local/share/octa/octa.db
```

このデータベースは Git にコミットされず、clone や remote には自動で同期されません。

マシン移行やバックアップが必要な場合は、このデータベースをバックアップしてください。

## コマンドを調べる

```sh
octa --help
octa issue --help
octa issue add --help
octa milestone --help
octa pr --help
octa wiki --help
octa config label --help
octa config label-group --help
octa config state --help
```

AI エージェントが octa CLI の機能、scope、JSON、保存場所を調べて利用するためのガイドは [`skills/octa`](skills/octa/SKILL.md) にあります。チーム固有の Issue 運用方針はこのガイドには含めません。

## 開発時の確認

```sh
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true TMPDIR=/private/tmp cargo test
```
