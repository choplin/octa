# Repository Scope and Storage

Read this reference only when an operation goes beyond the current repository or requires direct knowledge of where octa keeps its data.

## Repository scope

Normal commands operate on the Git repository containing the current working directory.

- Use `octa --repository <known-name> ...` to select another repository already known to octa.
- A unique name stored in both octa's database and repository-local Git configuration is the stable repository identity. The canonical Git common directory is its current location, so linked worktrees share one identity and dataset.
- Registration is normally implicit. Use `octa repository register --name <name> [path]` to choose a name explicitly, `octa repository set <name> --name <new-name>` to rename an identity, and `octa repository relocate <name> [path]` after moving a registered repository.
- Keep mutations and repository-specific filters scoped to one repository.

Use `--all-repositories` only for an explicit cross-repository overview. It is supported by these aggregate read-only commands:

```text
issue list
project list
```

With `--all-repositories`, `issue list` supports `--state`, `--state-type`, and `--all` because state configuration is global. The `--label`, `--project`, `--milestone`, `--related-to`, and `--unblocked` filters require one repository. The former `--repo` and `--all-repos` spellings remain compatibility aliases.

## Local storage

octa is local in the sense that normal operation does not require a hosted service. It stores records under the user's XDG data directory:

```text
$XDG_DATA_HOME/octa/octa.db
```

When `XDG_DATA_HOME` is unset, the default is:

```text
~/.local/share/octa/octa.db
```

The current storage implementation is SQLite. Use octa commands for normal reads and writes instead of accessing the database directly.

octa does not provide remote synchronization. Users may choose to version, copy, or synchronize the storage location with other tools; that policy is outside octa.

For backup or machine migration, stop processes using octa before copying storage. Do not copy only `octa.db` while writes may be in progress because SQLite sidecar files may contain current data.
