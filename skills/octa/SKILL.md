---
name: octa
description: Use and inspect the octa local collaboration CLI, including its read-only GraphQL query surface, Issues, Projects, Milestones, Pull Requests, Wiki pages, labels, states, repository scope, and JSON output. Use when Codex needs to discover octa commands, read octa records, or run octa CLI operations in a Git repository.
---

# Octa CLI

Use octa as a repository-scoped local collaboration tool that does not require a hosted service. Derive organizational process from the user's instructions or another policy source; this skill documents product behavior only.

## Start safely

1. Run inside the target Git repository.
2. Inspect `octa --help` and the relevant nested `--help` before constructing an unfamiliar command.
3. Prefer `--json` for machine consumption. Parse JSON instead of scraping human-readable tables.
4. Inspect current data before mutating it. Do not infer permission to create, edit, change state, lock, or delete records from a request to inspect or explain them.

## Choose a command surface

- Query: `octa query` executes a read-only GraphQL document from stdin or `--file`; prefer it when one read needs selected fields across related entity types.
- Issue: `octa issue ...` for create/list/show/set/unset/add/remove, comments, state transitions, and leases.
- Project: `octa project ...` for create/list/show/set/add/remove and state transitions.
- Milestone: `octa milestone ...`; every operation requires `--project`.
- Pull Request: `octa pull-request ...` for branch-associated records, comments, states, and explicit many-to-many Issue links through `add`/`remove`. The former `pr` spelling remains a compatibility alias.
- Wiki: `octa wiki ...` for pages, `[[slug]]` links, and backlinks.
- Configuration: `octa config issue state|label|label-group ...` and `octa config project label|label-group ...`; the record being configured is part of the command path.

Issue parent/child relations stay within one repository and do not require matching Project membership. A Project-less child initially inherits its parent's Project when the parent relation is set, but later Project changes and removal are independent.

An Issue lease is a non-expiring opaque credential returned once by `issue lock`. Pass it with `--lease` to protected Issue mutations and normal unlock; use `issue unlock --force` only to recover from a lost credential. List and show output expose only the `leased` boolean, never the credential.

Issue and Project label definitions are separate repository-defined opaque data. Do not infer an Issue type or other built-in taxonomy from particular names; octa reserves no operational label names. A `single` group only enforces mutual exclusion among labels explicitly created for the same target and group.

Read [commands-and-json.md](references/commands-and-json.md) when exact subcommands, constraints, or JSON shapes matter.

Read [scope-and-storage.md](references/scope-and-storage.md) when selecting another repository, aggregating across repositories, reasoning about worktrees, locating or moving stored data, or handling backup and synchronization.

Do not place team/process policy in this skill. Exclude Issue authoring standards, lifecycle ownership, grooming loops, model routing, completion or handoff note conventions, and internal-reference governance.
