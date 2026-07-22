# octa

Local, GitHub-style collaboration for a Git repository. octa provides Issues,
branch-linked pull-request discussions, Wiki pages, and labels without a hosted
service or web UI. It is a CLI for keeping collaboration context available to
people and agents working across multiple worktrees on the same machine.

## What it provides

- **Issues** — numbered discussions with comments, states, dependencies, labels,
  and an atomic exclusive lock for claiming work.
- **Pull requests** — numbered discussion entities tied to a Git branch; code and
  diffs remain in Git.
- **Wiki** — repository-scoped pages with links and backlinks.
- **Labels** — labels and single- or multi-select label groups.

## Install and run

This repository provides a Nix development shell with Rust, Cargo, and the
supporting tools. From the repository root:

```sh
nix develop
cargo build
cargo run -- --help
```

If Rust is already available locally, `cargo build` and `cargo run -- --help` are
enough. The built binary is at `target/debug/octa`.

## Quick start

Run octa from inside a Git repository. The current repository is the default
scope.

```sh
# Create and inspect an issue.
octa issue create --title "Document the release" --body "Capture the steps."
octa issue list
octa issue show 1
octa issue comment 1 --body "I will take this."

# Track state, dependencies, and a work claim.
octa issue set-state 1 in_progress
octa issue dep add 1 2       # issue 1 blocks issue 2
octa issue lock 1 --as docs-agent
octa issue unlock 1

# Open a branch-linked PR discussion.
octa pr create --title "Document the release" --branch docs/release
octa pr comment 1 --body "Ready for review."

# Keep repository knowledge in the Wiki.
octa wiki create --title "Release process" --body "..."
octa wiki list
octa wiki show release-process
```

Use `--json` on supported issue and PR commands when another program needs
machine-readable output. Run `octa --help` or, for example,
`octa issue --help` to see every available command.

## Labels and states

Create a mutually exclusive label group for one-of choices, or a multi-select
group for labels that can coexist:

```sh
octa label group --selection single priority
octa label create high --group priority
octa issue label 1 high

# The default issue states include open, in_progress, and closed.
octa state list
octa state add blocked --starting
```

## Repository scope and storage

octa stores collaboration data in one SQLite database at
`$XDG_DATA_HOME/octa/octa.db`. When `XDG_DATA_HOME` is unset, it uses
`~/.local/share/octa/octa.db`. This database is not committed to a repository.

The default repository identity comes from Git's common directory, so every
worktree of the same repository shares its octa data. To work outside the
current repository:

```sh
# Target a named repository in the store.
octa --repo other-repository issue list

# Aggregate read-only views across every stored repository.
octa --all-repos issue list
```

## Current status

The v1 CLI surface for Issues, PR discussions, Wiki pages, labels, and states is
implemented. An agent-oriented skill layer is planned but is not yet part of this
repository.

## Scope

octa is a local collaboration layer, not a replacement for Git hosting. It does
not provide a full web UI, remote synchronization, authentication, or
multi-person real-time collaboration.
