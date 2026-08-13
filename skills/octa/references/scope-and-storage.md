# Repository Scope and Storage

Read this reference only when an operation goes beyond the current repository or requires direct knowledge of where octa keeps its data.

## Repository scope

Normal commands operate on the Git repository containing the current working directory.

- Use `octa --repo <known-name> ...` to select another repository already known to octa.
- Worktrees that share the same Git common directory share one octa repository identity and dataset.
- Keep mutations and repository-specific filters scoped to one repository.

Use `--all-repos` only for an explicit cross-repository overview. It is supported by these aggregate read-only commands:

```text
issue list
project list
pr list
wiki list
```

With `--all-repos`, `issue list` supports the generic `open`, `closed`, and `all` state filters, plus status type and priority. Named states and the `--label`, `--project`, `--milestone`, `--related-to`, and `--unblocked` filters require one repository.

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
