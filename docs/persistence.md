# Persistence

octa keeps all collaboration data for one OS user in one SQLite database while
presenting repository-scoped records to ordinary commands. The physical store
is centralized so linked worktrees and cross-repository views do not require
opening and merging multiple databases; repository foreign keys preserve the
logical boundary inside that store.

## Physical store and logical scopes

The database is `$XDG_DATA_HOME/octa/octa.db`, falling back to
`~/.local/share/octa/octa.db`. `Store` opens it with WAL journaling and a
five-second busy timeout. SQLite supplies cross-process serialization; octa
adds operation-specific transactions where several statements form one
meaningful change.

Most entity tables carry `repository_id`, and repository-local identifiers are
composite keys: `(repository_id, issue_number)`, `(repository_id, project_id)`,
and analogous forms. Repository scope is therefore part of identity, not an
optional query label.

Configuration is intentionally global:

- Issue states, their per-type defaults, labels, and label groups govern every
  repository.
- Project states, their per-type defaults, labels, and label groups do the same.

The command layer rejects repository selectors on `octa config` rather than
silently pretending that configuration is repository-local.

## Repository ownership

`repositories` maps a stable, user-facing name to the repository's current
canonical root path. The name is also stored as `octa.repositoryName` in local
Git configuration, which is shared by linked worktrees through the common Git
directory. This lets a relocated repository keep its database identity without
treating the old path as permanent.

Current scope resolves from the canonical Git common directory, then derives
the repository root. Named scope resolves an existing database row. All scope
is permitted only for operations designed to aggregate; methods that need one
repository fail through `Store::repository_id`.

The complete registration, collision, worktree, and relocation rules live in
[`design/repository-identity-and-scope.md`](design/repository-identity-and-scope.md).

## Data integrity

`migrations/0001_init.sql` is more than table creation; it holds invariants
that must survive every caller:

- state rows cannot be retyped, every populated state type retains one default,
  and entities cannot reference an unconfigured state;
- parent edges cannot point to self or form a cycle;
- symmetric relations are stored once in low/high canonical order;
- an Issue Milestone must belong to the same Project as the Issue;
- relationship foreign keys include `repository_id`, preventing cross-repository
  edges;
- one `issue_leases` row per Issue makes simultaneous ownership
  unrepresentable.

The application layer still validates early to produce useful errors, but
schema constraints are the final authority for representations that must remain
valid under concurrency or future callers.

## Write ownership and transactions

`src/sql` owns database statements. `src/app` decides which statements form a
workflow operation and opens or commits the transaction through the SQL
adapter. Examples include a lease validation plus protected mutation, a parent
change plus optional Project inheritance, and a Milestone assignment plus
Issue timestamp update.

Single-statement operations rely on SQLite atomicity when no wider workflow
invariant is involved. Repository-local numbers are allocated by one
`INSERT ... SELECT COALESCE(MAX(...)) + 1 ... RETURNING` statement rather than
by a read followed by a write.

## Database lifecycle

The migration is embedded in the binary through `sqlx::migrate!`. SQLx offline
metadata under `.sqlx/` allows checked query macros to build without a live
database. A schema or checked-query change is incomplete until that metadata
matches the source.

During pre-release development the repository maintains one rewritten
`migrations/0001_init.sql`; it does not append numbered migrations. A live
database must therefore be replaced from a logical dump after the matching
binary is merged and installed, rather than opened with a binary that embeds a
different checksum. The exact procedure and compatibility constraints are in
[`design/schema-evolution.md`](design/schema-evolution.md).
