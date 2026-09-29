# Repository Identity and Scope

How does octa make one repository identity survive linked worktrees and path
moves while keeping repository names unambiguous in a user-global database?

## The rule

A repository row has an internal integer ID, a unique user-facing name, and one
current canonical root path. The durable locator shared with Git is the name,
stored as `octa.repositoryName` in repository-local Git configuration. The path
is current location metadata and may change through `repository relocate`.

For current scope, octa asks Git for `--git-common-dir`, canonicalizes it, and
derives the root:

- a common directory ending in `.git` maps to its parent;
- a bare repository or separate Git directory that does not end in `.git`
  remains its own root.

Because linked worktrees report the same common directory, they read the same
local Git configuration and resolve the same octa repository.

## Registration

Opening current scope first resolves Git without changing the database. It then
applies these cases:

1. A row already exists at the canonical path. Its name must match
   `octa.repositoryName`; a missing or conflicting Git marker is an error.
2. No row exists at the path, but the configured name belongs to another path.
   octa reports that the repository moved and requires explicit relocation.
3. Neither locator exists. octa validates a unique name, writes it to local Git
   configuration, and inserts the repository row.

If insertion fails after writing Git configuration, octa restores the previous
marker. If another process won a concurrent registration, cleanup preserves
the winning durable name rather than deleting it.

`repository register` makes the name explicit. `repository set --name` changes
both stores with stale-snapshot protection. `repository relocate` changes the
path only after the named row and the repository at the new location have been
validated.

## Scope semantics

`RepositoryScope` has three states:

- `Current` resolves or registers the Git repository containing the working
  directory;
- `Named` resolves an existing row by its unique name;
- `All` carries no active repository ID and is valid only for explicitly
  aggregate operations.

The `Store` owns the resolved scope. A single-repository method must obtain its
ID through `repository_id()`, so an accidental call under `All` fails before it
can issue an unscoped entity query.

Repository listing is store-wide and intentionally opens with `All`, allowing
it to run outside Git without registering the caller's directory. Configuration
commands are also store-wide, but reject repository selector flags because
configuration is global rather than aggregate repository data.

## Why not path-only or remote identity?

A working-tree root changes between linked worktrees, and a canonical path
changes when the repository moves. Either makes location masquerade as durable
identity. A remote URL is optional, mutable, and not unique when several
remotes exist. The shared local Git name supplies stable identity without
requiring a remote; the canonical path remains useful for collision detection
and relocation.

## Verification

Changes to this contract must cover normal repositories, linked worktrees,
bare/separate Git directories, moves, duplicate names and paths, mismatched Git
markers, failure cleanup, concurrent registration, and store-wide listing
outside Git. Unit coverage for root derivation lives beside `src/store/mod.rs`;
black-box registration and relocation behavior lives in `tests/cli.rs`.
