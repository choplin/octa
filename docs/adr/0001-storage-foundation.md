# ADR 0001: ストレージ基盤（グローバルストア・per-repo スコープ・エンジン）

- Status: Accepted
- Date: 2026-07-22

## Context

octa は複数セッション・複数エージェントの協働データ（Issue / PR / Wiki / ラベル）を、外部サービスもフル Web UI もなしにローカルで保持する。構想フェーズで以下は確定済みで再オープンしない。

- **単一のユーザーグローバルなストア**（XDG data dir 配下）に全リポジトリの全エンティティを保持する。
- 全エンティティは**論理的にリポジトリにスコープ**する。cwd が既定スコープを解決し、明示フラグで別 repo・全 repo を対象にできる。
- worktree 横断を自然に満たす。複数エージェントの並行アクセスに安全。リポジトリにはコミットしない。

本 ADR は、その上で未決だった 3 点 — (1) エンジン、(2) repo identity のキー付け、(3) 並行制御・原子性 — を確定する。

## 参考にした現状（拘束はされない）

動く Rust 試作があり、Issue / コメントを sqlx + SQLite（コンパイル時クエリ検証・`.sqlx` オフラインキャッシュ）で保存していた。ただし物理位置は `.git/octa/octa.db`（common git dir 配下）で、これは確定モデル（グローバルストア）より前の位置付け。試作は参考であり、エンジン継続も作り直しも自由。

## 決定

### 1. エンジン: グローバル単一 SQLite DB ＋ `repo_id` スコープ列

- 物理位置: `$XDG_DATA_HOME/octa/octa.db`（未設定時は `~/.local/share/octa/octa.db`）。全 repo で 1 ファイル。
- 各エンティティ行は `repo_id` 列を持ち、論理スコープを表す。既定スコープ（cwd の repo）は単なる `WHERE repo_id = ?`、全 repo 横断はその句を外すだけで、横断が特別扱いにならない。
- sqlx を継続。`cargo sqlx prepare` でオフラインキャッシュを再生成し、コンパイル時クエリ検証を保つ。

**却下:**
- **per-repo DB ファイル**（XDG 配下に repo ごとの `.db`）: 横断俯瞰が N ファイルの open ＋マージになり不自然。既定 per-repo・横断のどちらも「1 DB ＋ WHERE 句」で得られる単一 DB 案の利点を捨てる。
- **Markdown ファイル**: 多エージェントの並行原子書き込みが弱く、関係・依存グラフのクエリを自前実装することになる。リポジトリにコミットしない以上、git 親和という Markdown の利点も薄い。

### 2. repo identity: canonical git-common-dir パス

「どの repo か」の同定キーは `git rev-parse --git-common-dir` を正規化した絶対パスとする。`repos` テーブルがこのキーから `repo_id`（整数）へマップし、初回利用時に upsert する。

- **no-remote repo を扱える**（remote URL に依存しない）。
- worktree 群は同一 common-dir を共有するため、**単一 identity に自然に解決**される（main worktree は相対 `.git`、linked worktree は絶対パスを返すが、正規化で一致する）。

**却下:**
- **repo-root path**: linked worktree ごとに root が異なり、単一 identity に解決できない。
- **remote URL**: no-remote repo で不可。remote は複数持てる・変更されうるため安定キーにならない。

### 3. 並行制御・原子性: WAL ＋ busy_timeout、原子性は単文/トランザクション

- SQLite を **WAL モード**・**`busy_timeout`（5s）** で開く。個人ツール規模の並行度はこの範囲で捌ける。
- **per-repo 連番**は「`MAX(number)+1` を副問い合わせで計算する単一 INSERT 文」で採番する。単一 SQL 文は書き込みロック下で原子実行されるため、並行 create でも番号が重複しない。global AUTOINCREMENT は使わない（番号は per-repo）。
- **原子的 lock**（排他取得）は、issue 行の `locked_by` 列への**条件付き UPDATE**（`WHERE number=? AND locked_by IS NULL`、compare-and-set）で実装する。`changes()` が 1 なら取得成功、0 なら既に他者が保持。SQLite の単文原子性により 2 体の同時取得は起こらない。
- 複数手順の変更はトランザクションに包む。

**却下:**
- **ファイルロック（flock 等）のみ**: SQLite 自身の WAL ＋条件付き UPDATE で原子性が得られるのに、別レイヤのロックを重ねる必要がない。

## 帰結

- Issue 番号採番は per-repo（`repo_id` ごとの連番）。
- スキーマは全テーブルに `repo_id` を持ち、`repos` が identity → `repo_id` を管理する。
- この基盤の上に Issue / PR / Wiki / ラベルの各プリミティブが乗る。
- データはリポジトリと一緒に運ばれない（マシン間同期・多人数は非ゴールと整合）。将来ポータビリティが必要になれば storage モデルの変更が要る（既知のトレードオフ）。
