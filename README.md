# octa

octa is a local Issue collaboration CLI for individual developers coordinating multiple AI agents, worktrees, and development sessions in a Git repository.

It keeps work, dependencies, ownership, and handoff context in a local SQLite database. No hosted service or account is required.

## Why octa?

- **Keep work context between sessions.** Issue bodies and comments preserve decisions, progress, and the next concrete step.
- **Prevent duplicate ownership.** Atomic leases let one agent or session claim an Issue before changing it.
- **Keep repositories separate.** Issues, Projects, and Milestones are scoped to a Git repository, while linked worktrees share the same records.
- **Support people and automation.** Human-readable output, JSON output, and a read-only GraphQL query surface use the same local data.

octa 0.1.0 supports an Issue-centered workflow. Pull Request and Wiki workflows are intentionally deferred until they have been exercised and released separately.

## Installation

octa 0.1.0 requires Git. Installing with Cargo also requires Rust 1.90 or later. The distribution commands below apply after the corresponding 0.1.0 package or GitHub Release has been published.

### Homebrew

Homebrew is the recommended installation method on macOS and Linux. Releases
that include the Homebrew channel can be installed and upgraded with:

```sh
brew install choplin/tap/octa
brew upgrade octa
```

### Nix

The repository flake builds octa from source in a Nix environment, without
requiring a separately installed Rust toolchain:

```sh
nix profile install github:choplin/octa#octa
octa --version
```

The flake does not currently use a project binary cache, so the first install
builds octa and its Rust dependencies locally.

### cargo-binstall

For releases published with cargo-binstall metadata, Rust users can install the
matching prebuilt GitHub Release archive instead of compiling octa locally:

```sh
cargo binstall octa-cli
octa --version
```

### Cargo

The 0.1.0 crates.io package is named `octa-cli`, and it installs a binary named `octa`.

```sh
cargo install octa-cli --version 0.1.0 --locked
octa --version
```

Make sure `~/.cargo/bin` is on your `PATH`.

### GitHub Releases

The 0.1.0 release matrix contains checksummed archives for:

- Apple Silicon macOS (`aarch64-apple-darwin`)
- Intel macOS (`x86_64-apple-darwin`)
- x86_64 Linux with glibc (`x86_64-unknown-linux-gnu`)

Download the archive and matching `.sha256` file for your platform from the
[GitHub Releases page](https://github.com/choplin/octa/releases), verify the
checksum, then place the extracted `octa` binary on your `PATH`.

Windows and Linux ARM binaries are not part of the 0.1.0 release.

Maintainers can find the release ownership, dry-run, and publication procedure
in [the release automation design](docs/design/release-automation.md).

## Quick start

Run octa inside the Git repository whose work you want to track. The repository is registered automatically when octa first stores data for it.

```sh
cd path/to/your-repository

octa issue open \
  --title "Document the release process" \
  --body "Record the required checks and handoff notes."

octa issue list
octa issue show 1
```

A successful `issue open` prints `#1`. The final `issue show` command confirms the stored Issue:

```text
#1 Document the release process (open)
project: No Project
milestone: No Milestone

Record the required checks and handoff notes.
```

Claim the Issue before changing it. `issue lock` prints a three-word lease ID once; keep it in the current shell and pass it to protected mutations.

```sh
LEASE=$(octa issue lock 1)
octa issue start 1 --lease "$LEASE"
octa issue comment add 1 --body "Started by checking the existing release steps."
```

To hand unfinished work to another session, record the current state and next step, then release the lease. The Issue remains in progress.

```sh
octa issue comment add 1 \
  --body "Handoff: checks are documented; next, verify a clean installation."
octa issue unlock 1 --lease "$LEASE"
```

The next session can inspect the comments and claim the same Issue.

```sh
octa issue show 1
LEASE=$(octa issue lock 1)
```

When the work is accepted, close the Issue and release its lease.

```sh
octa issue close 1 --lease "$LEASE"
octa issue unlock 1 --lease "$LEASE"
```

If a lease ID is irretrievably lost, `octa issue unlock 1 --force` invalidates it. Use forced unlock only after confirming that no active session still owns the work.

Long Markdown can be read from a file or from standard input instead of crossing a shell argument boundary.

```sh
octa issue open --title "Release notes" --body-file notes.md
printf '%s\n' 'Handoff: validation is pending.' | \
  octa issue comment add 2 --body-file -
```

## Working with Issues

An Issue contains a title, body, state, comments, labels, relations, and an optional lease.

### States

`issue start`, `close`, and `reopen` move an Issue to the configured default state for the corresponding state type. Use `issue set --as <STATE>` when a workflow needs an exact configured state.

```sh
octa issue list
octa issue list --state-type "open,in progress"
octa issue list --all
octa config issue state list
```

With no state selector, `issue list` omits closed Issues. State and label configuration is shared across the local octa store; repository records remain repository-scoped.

### Labels and relations

Labels classify Issues. Relations record ordering or context without hiding it in prose.
The following commands continue from the Quick start by creating Issue 2 and making Issue 1 its blocker.

```sh
octa issue open --title "Publish 0.1.0"
octa config issue label list
LEASE_2=$(octa issue lock 2)
octa issue add 2 --blocker 1 --lease "$LEASE_2"
octa issue add 2 --related 1 --lease "$LEASE_2"
octa issue list --unblocked
```

`--blocker 1` means Issue 2 is blocked by Issue 1. Parent and child relations group a small set of deliverables; they do not imply execution order.

### Projects and Milestones

A Project represents a finite outcome. Ordered Milestones divide that outcome into stages.

```sh
octa project create --name "Publish 0.1.0"
octa milestone create \
  --project "Publish 0.1.0" \
  --name "Release candidate" \
  --status active \
  --position 1

octa issue open \
  --title "Verify the packaged binary" \
  --project "Publish 0.1.0" \
  --milestone "Release candidate"
```

Use `octa project list --active` to show only Projects whose configured state is not closed.

### Terminal and machine-readable views

The Issue TUI is a read-only two-pane browser.

```sh
octa issue tui
```

Commands that support JSON accept `--json`. Issue list entries include their labels.

```sh
octa issue list --all --json
octa issue show 1 --json
octa project list --active --json
```

`octa query` executes read-only GraphQL. Inspect the public schema before building a query that will be kept in automation.

```sh
octa query --schema

octa query <<'GRAPHQL'
{
  issues(limit: 20) {
    number
    title
    state
    leased
    labels { name }
  }
}
GRAPHQL
```

## Repository scope

octa normally uses the Git repository containing the current directory. All linked worktrees of that repository share its octa records.

Use `--repository <NAME>` to read or change another registered repository. Supported read-only commands can aggregate repositories with `--all-repositories`.

```sh
octa repository list
octa --repository another-repository issue list
octa --all-repositories issue list --all
```

Repository registration is usually automatic. The explicit repository commands handle naming conflicts and repositories that have moved.

```sh
octa repository register --name another-repository /path/to/repository
octa repository relocate another-repository /new/path/to/repository
```

## Data and backups

octa stores all repository records for the current user in one SQLite database:

```text
$XDG_DATA_HOME/octa/octa.db
```

When `XDG_DATA_HOME` is unset, the database is stored at:

```text
~/.local/share/octa/octa.db
```

The database is not committed to Git and is not synchronized automatically.

The source repository provides logical backup and restore scripts. Stop every process using octa before a restore. Run the scripts from the source checkout whose schema should own the restored database, and install that same octa version before replacing a live database.

```sh
# Requires sqlite3; prints the new dump path.
scripts/db-dump

# Also requires sqlx-cli. The previous database is archived on success.
scripts/db-restore /path/to/octa-data.sql
```

`db-restore` builds a new database from the repository's current migration, loads the dump in a transaction, and checks foreign-key and SQLite integrity before replacing the live database.

## Current boundaries

octa 0.1.0 is designed for one developer coordinating local agents and sessions. It does not provide:

- Git hosting or remote synchronization
- a hosted service, web UI, or authentication
- real-time multi-user collaboration
- public Pull Request or Wiki commands
- prebuilt Windows or Linux ARM binaries

Internal Pull Request and Wiki storage is retained for staged future support, but it is not part of the 0.1.0 public interface.

## Command discovery

The CLI help is the command reference for the installed version.

```sh
octa --help
octa issue --help
octa issue add --help
octa project --help
octa milestone --help
octa config --help
octa query --schema
```

AI agents can use the repository's [octa CLI guide](skills/octa/SKILL.md) for machine-readable output, mutation safety, leases, and repository-scope behavior. Workflow policy remains separate from the CLI's product contract.

## Development

This repository provides a Nix development environment.

Developer architecture, design contracts, and decision history start at
[`docs/architecture.md`](docs/architecture.md). The organization policy for
that documentation is in [`docs/README.md`](docs/README.md).

```sh
nix develop
cargo build
```

Required checks:

```sh
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true TMPDIR=/private/tmp cargo test
```

## License

octa is licensed under the [MIT License](LICENSE-MIT).
