---
name: octa
description: Use and inspect the octa local collaboration CLI, including repository scope, Issues, Projects, Milestones, Pull Requests, Wiki pages, labels, states, JSON output, and local storage. Use when Codex needs to discover octa commands, read octa records, or run octa CLI operations in a Git repository.
---

# Octa CLI

Use octa as a repository-scoped local data tool. Derive organizational process from the user's instructions or another policy source; this skill documents product behavior only.

## Start safely

1. Run inside the target Git repository unless using `--repo`.
2. Inspect `octa --help` and the relevant nested `--help` before constructing an unfamiliar command.
3. Prefer `--json` for machine consumption. Parse JSON instead of scraping human-readable tables.
4. Inspect current data before mutating it. Do not infer permission to create, edit, change state, lock, or delete records from a request to inspect or explain them.

Install from this repository with `cargo install --path .`. During development, use `./target/debug/octa`.

## Select repository scope

- Omit global scope flags to use the current Git repository.
- Use `octa --repo <known-name> ...` to select another registered repository.
- Use `octa --all-repos ...` only on commands that support aggregate read-only access.
- Treat worktrees sharing the same Git common directory as one octa repository and one dataset.
- Keep mutations and repository-specific filters scoped to one repository.

## Choose a command surface

- Issue: `octa issue ...` for create/list/show/set/unset/add/remove, comments, state transitions, and locks.
- Project: `octa project ...` for create/list/show/set/add/remove and state transitions.
- Milestone: `octa milestone ...`; every operation requires `--project`.
- Pull Request: `octa pr ...` for branch-associated records, comments, states, and explicit many-to-many Issue links through `add`/`remove`.
- Wiki: `octa wiki ...` for pages, `[[slug]]` links, and backlinks.
- Configuration: `octa config state|label|label-group ...`; label commands require `--target issue|project`.

Issue parent/child relations stay within one repository and do not require matching Project membership. A Project-less child initially inherits its parent's Project when the parent relation is set, but later Project changes and removal are independent.

Issue and Project label definitions are separate repository-defined opaque data. Do not infer an Issue type or other built-in taxonomy from particular names; octa reserves no operational label names. A `single` group only enforces mutual exclusion among labels explicitly created for the same target and group.

Read [commands-and-json.md](references/commands-and-json.md) when exact subcommands, constraints, or JSON shapes matter.

## Handle storage and backup

octa stores data in one local SQLite database at `$XDG_DATA_HOME/octa/octa.db`, or `~/.local/share/octa/octa.db` when `XDG_DATA_HOME` is unset. The database is outside Git and is not remotely synchronized. Back up that database for recovery or machine migration, avoiding writes while copying it.

Do not place team/process policy in this skill. Exclude Issue authoring standards, lifecycle ownership, grooming loops, model routing, completion or handoff note conventions, and internal-reference governance.
