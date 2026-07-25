# octa

**チーム開発グレードの協働を、個人の AI エージェント駆動開発にローカルでもたらす CLI。**

octa は、1台のマシンで複数の AI エージェントと複数の開発セッションを並行して動かす個人開発者のための、ローカル協働基盤です。

Issue、Pull Request、Wiki という GitHub 風のメンタルモデルで、リポジトリが何を目指し、何が残っているかを、セッションをまたいで残します。

データは外部サービスに送信せず、ローカルの SQLite に保存されます。

## octa が解決すること

AI エージェントに作業を分けると、判断の背景、未処理の作業、次の担当者へ渡すべき文脈がセッションごとに散ります。

octa は、その情報をリポジトリ単位で持続する記録にします。

- **Issue**：状態、依存関係、コメント、ラベル、原子的な lock を持つ作業記録です。
- **Pull Request**：Git ブランチに紐づく議論と状態の記録です。コードと diff は Git 側に残ります。
- **Wiki**：方針や手順を残すページです。`[[slug]]` によるリンクと backlink を使えます。
- **Label と state**：各プロジェクトの分類と作業フローを設定できます。

octa は Git hosting、Web UI、リモート同期、認証、多人数のリアルタイム協働を提供しません。

同じマシン上で動く複数の worktree、エージェント、セッションの調整に焦点を絞っています。

## 前提条件

- Git リポジトリの中で実行すること。
- Rust と Cargo を使えること。

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

作業を始めるエージェントまたはセッションは lock を取得できます。

```sh
octa issue lock 1 --as docs-agent
octa issue comment 1 --body "着手しました。"
```

完了後は lock を外し、状態を更新します。

```sh
octa issue unlock 1 --as docs-agent
octa issue close 1
```

`OCTA_ACTOR` を設定すると、`--as` を省略したときの lock 保持者名に使われます。

```sh
export OCTA_ACTOR=docs-agent
octa issue lock 1
```

## Issue で作業を調整する

Issue は番号、本文、コメント、状態、依存関係、ラベルを持ちます。

### 状態と一覧

既定の状態は `open`、`in_progress`、`closed` です。

```sh
octa issue set-state 1 in_progress
octa issue list --state open
octa issue list --state closed
octa issue list --state all
```

Linear の Issue 一覧と詳細の代わりに、read-only の2ペインTUIも使えます。
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
`project list` は既定で completed / canceled を含む全 Project を返し、各 tally
も canceled を含む全 Issue を数えます。作業中の Project だけが必要な場合は
`project list --active` と明示します。Project は priority 1〜4 の順、その後に
0（None）の順で表示されます。

```sh
octa project create --name "CLI を公開する"
octa project list
octa project list --active

octa project milestone create "CLI を公開する" \
  --name "Public beta" \
  --description "利用者向けbetaを公開する段階" \
  --status active \
  --position 1 \
  --target-date 2026-09-01

octa project milestone list "CLI を公開する"
octa project milestone show "CLI を公開する" "Public beta"
octa project milestone edit "CLI を公開する" "Public beta" \
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
clear します。

```sh
octa issue milestone set 1 "Public beta"
octa issue list --project "CLI を公開する" --milestone "Public beta"
octa issue milestone clear 1
```

Issue の親子関係は同じリポジトリ内で設定でき、Project の所属とは独立しています。
親子は異なる Project に所属でき、片方だけが Project に所属していても構いません。
Project のない既存 Issue に親を設定した時は、その時点の親の Project を初期値として
継承しますが、その後は親子それぞれの Project を変更または解除できます。

```sh
octa issue parent set 2 1
octa issue project set 2 "別の Project"
octa issue project clear 1
```

Project 内の Milestone が設定されている Issue だけは、従来どおり先に Milestone を
clear してから Project を変更または解除します。

新規リポジトリに自動作成される状態名は、互換性のための `open`、
`in_progress`、`closed` だけです。プロジェクト固有のworkflow状態は自由に追加できます。

```sh
octa state add Backlog --type backlog
octa state add Todo --type unstarted
octa state add "In Progress" --type started
octa state add "In Review" --type started
octa state add Done --type completed
octa state add Canceled --type canceled
octa state add blocked
octa state list
```

`--starting` と `--terminal` は状態の入口・終端を示すフラグです。

初期状態では `open` が入口、`closed` が終端です。status type は
`backlog`、`unstarted`、`started`、`completed`、`canceled` の5分類です。
これらのstatus typeは一般的な分類であり、特定の状態名を要求しません。
Backlog、Todo、In Progress、In Review、Done、Canceled などのworkflow名は、
必要なリポジトリで上記のように追加します。旧バージョンで作成済みのworkflow状態や
その他のcustom/legacy stateと、それらを参照するIssueはmigration後も削除・改名されません。

現在の CLI では、後から追加した状態を close / reopen の既定遷移先に変更する操作はありません。

### 依存関係

Issue 1 が Issue 2 をブロックする関係を作るには、次を実行します。

```sh
octa issue dep add 1 2
octa issue show 1
octa issue show 2
```

まだ終端状態ではない blocker を持たない作業は、`--unblocked` で一覧できます。

```sh
octa issue list --unblocked
```

依存を削除するには `dep rm` を使います。

```sh
octa issue dep rm 1 2
```

順序を持たない関連 Issue は `relate` で結びます。同じ組を逆順で追加しても一件だけ保存され、`--related-to` で候補を絞れます。

```sh
octa issue relate add 1 2
octa issue list --related-to 1
octa issue relate rm 2 1
```

### ラベル

ラベルは単独でも使えます。
ラベル名とグループ名はリポジトリごとに自由に決められ、octa が予約する分類名や
`impl` / `design` / `research` のような特別扱いされるラベルはありません。

```sh
octa label create documentation
octa issue label 1 documentation
octa issue unlabel 1 documentation
```

`single` グループでは、同じグループのラベルを一つだけ付けられます。

`multi` グループでは、同じグループのラベルを複数共存させられます。

```sh
octa label group priority --selection single
octa label create high --group priority
octa label create low --group priority
octa issue label 1 high

octa label group area --selection multi
octa label create cli --group area
octa label create storage --group area
octa issue label 1 cli
octa issue label 1 storage
```

## Pull Request の議論を残す

octa の Pull Request は、ブランチに紐づく番号付きの議論エンティティです。

コードと diff は Git が扱い、octa は状態とコメントを保持します。

```sh
octa pr create \
  --title "リリース手順を追加する" \
  --branch docs/release-process \
  --body "Wiki と README を更新する。" \
  --issue 1

octa pr comment 1 --body "確認をお願いします。"
octa pr show 1
octa pr close 1
```

既存 PR は作成時と同じ形のまま利用でき、必要になった時だけ Issue と明示的に link できます。
1つの Issue に複数の PR、1つの PR に複数の Issue を link できます。同じ組は重複保存されません。

```sh
octa pr link 1 2
octa issue show 1
octa pr unlink 1 2
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

## JSON 出力

自動化やエージェントから利用する場合は、対応するコマンドに `--json` を付けます。

```sh
octa issue create --title "調査する" --json
octa issue list --state all --json
octa issue show 1 --json
octa pr list --state all --json
octa wiki show release-process --json
octa label list --json
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
octa --all-repos issue list --state all
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
octa issue dep --help
octa pr --help
octa wiki --help
octa label --help
octa state --help
```

## Reconstructed pre-separation policy baseline

- Reserved label names infer an Issue taxonomy shown by the TUI.
- Repositories receive Backlog/Todo/In Progress/In Review/Done/Canceled with persisted workflow groups/ranks, and Project lists are implicitly active-only.
- Composite transitions require a completion note for terminal states.
- Parent and child Issues must remain in the same Project.
- Issue-to-PR ownership and Issue JSON projections are singular.

AI エージェントが octa CLI の機能、scope、JSON、TUI、保存場所を調べて利用するためのガイドは [`skills/octa`](skills/octa/SKILL.md) にあります。チーム固有の Issue 運用方針はこのガイドには含めません。

## 開発時の確認

```sh
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true TMPDIR=/private/tmp cargo test
```
