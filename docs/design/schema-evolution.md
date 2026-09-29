# Schema Evolution Before the First Release

How is octa's SQLite schema changed while the project still rewrites its
initial migration and already has dogfood data worth preserving?

## The rule

Before the first released schema is frozen, edit only
`migrations/0001_init.sql`; do not add another numbered migration. Treat the
file as the complete schema a fresh installation should have, and update the
corresponding Rust code, tests, and SQLx offline metadata in the same change.

Do not apply a rewritten migration to an existing live database. SQLx records
the applied migration checksum, and an installed binary that embeds another
version of `0001_init.sql` will reject that database. Build a fresh database
from the new migration and restore logical data into it instead.

## Development and verification

Use a disposable database for schema validation. The repository's required
checks are:

```sh
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true TMPDIR=/private/tmp cargo test
```

When checked queries change, regenerate `.sqlx/` metadata against a disposable
database created from the current migration. Include test targets so queries
behind `cfg(test)` are represented. Machine-local Cargo patch configuration,
when used for Urushi dogfooding, must not be copied into the repository.

## Replacing a live database

Replacing the dogfood database happens only after the schema change has been
reviewed, merged to the base branch, and the matching octa binary has been
installed:

1. stop every process that can use octa;
2. run `scripts/db-dump` and confirm the printed dump file is non-empty;
3. run `scripts/db-restore <dump.sql>` from the checkout containing the installed
   schema;
4. verify representative reads and record counts before resuming normal use.

The dump must be taken immediately before restore, not earlier during
implementation, or intervening Issues and comments could be lost.

`db-restore` creates a temporary database, applies the current migration,
imports data in a transaction, runs `foreign_key_check` and `integrity_check`,
and only then replaces the live file. The previous database and SQLite sidecars
are retained beside it with a `pre-restore` timestamp.

This operation replaces user data and therefore requires explicit operator
approval. It is not part of implementing or testing a schema change on a work
branch.

## Dump compatibility

`db-dump` uses SQLite's data-only dump and excludes SQLx's migration table.
Generated `INSERT` statements do not name columns, so changing a table's column
count or order can make the dump incompatible even when the logical data still
fits.

When that happens, keep the original dump, copy it to a temporary working file,
and explicitly transform only the affected `INSERT` statements. Do not add a
generic converter or silently discard fields. Re-run restore with the converted
copy so all normal integrity checks still guard the replacement.

## Why rewrite one migration?

During initial development, a single clean schema keeps fresh installations
free from a sequence of abandoned intermediate designs. Once released users
depend on migration history, rewriting an applied migration is no longer a
valid evolution strategy; this document must then be replaced with the released
append-only migration policy.
