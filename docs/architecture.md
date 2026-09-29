# Architecture

This is the starting point for developers changing octa. octa is one Rust
binary that turns a user-global SQLite database into repository-scoped Issue
collaboration through a CLI, JSON output, a read-only GraphQL surface, and a
read-only TUI.

The product [`README`](../README.md) explains the user workflow. This document
explains where behavior belongs, how the layers depend on each other, and which
invariants cross those layers. Exact rules and rationale live under
[`design/`](design/); [`decision-log.md`](decision-log.md) records when the
important choices changed.

## The execution path

Every invocation enters through `src/main.rs`, parses the command, opens one
`Store`, and dispatches to a surface-specific handler.

```text
arguments / stdin / terminal events
                |
                v
             src/cli
                |
                v
        Store facade (src/store)
                |
                v
     workflow policy (src/app)
                |
                v
       SQL adapters (src/sql) ------> SQLite
                |
                v
       domain projections (src/domain)
                |
                +------> text / JSON / TUI output

GraphQL document --> src/query planner ----------> SQLite JSON projection
```

The ordinary command path deliberately separates transport, policy, and
persistence:

- `src/cli` owns command grammar, cross-option validation, input acquisition,
  dispatch, and human or JSON presentation. It does not own business rules or
  SQL.
- `src/store` is the repository-scoped application facade. It resolves the
  active repository once and gives CLI and TUI callers task-oriented methods.
- `src/app` owns workflow validation and coordinates multi-step changes. It is
  the place for rules such as project/milestone consistency, parent-cycle error
  translation, and lease-protected mutation boundaries.
- `src/sql` owns SQLx queries, transactions, private row shapes, and immediate
  conversion into domain projections. SQL rows do not escape this layer.
- `src/domain` owns serialized output shapes and closed vocabularies such as
  `StateType`, not I/O or database access.

Dependencies flow downward through those responsibilities. A new CLI command
may expose an existing store operation; a new workflow rule belongs in `app`;
a new persistence mechanism belongs in `sql`. Avoid placing policy in output
formatting or teaching SQL adapters about CLI syntax.

## Store opening is part of command behavior

`Store::open` resolves the database path, opens SQLite in WAL mode, runs the
embedded migration, seeds global Issue and Project state configuration when it
is absent, and then resolves the requested repository scope. Most commands
therefore may initialize persistent state before doing their own work.

The exceptions are deliberate:

- help augmentation uses `open_existing_pool`, so asking for help never creates
  a database or runs migrations;
- `octa repository ...` opens the store with all-repository scope and can run
  outside a Git worktree;
- CLI validation that can reject a command without storage runs before
  `Store::open`.

The storage topology and lifecycle are covered in
[`persistence.md`](persistence.md). The exact repository identity contract is
in [`design/repository-identity-and-scope.md`](design/repository-identity-and-scope.md).

## Issue collaboration model

Issues are the main public collaboration record in 0.1.0. An Issue has a
repository-local number, configured state, body, comments, labels, optional
Project and Milestone membership, directed blocking edges, a parent/child edge,
symmetric related edges, and at most one active lease.

The relationships intentionally keep different meanings:

- dependencies express execution order (`blocks` / `blocked_by`);
- parentage groups work and must remain acyclic;
- related edges add context without order or hierarchy;
- Project and Milestone membership group an Issue into a finite outcome and an
  ordered stage of that outcome.

SQLite composite keys and foreign keys keep every relationship inside one
repository. Application services add rules that require richer context, such
as inheriting a parent's Project or rejecting a Project change while a
Milestone is attached.

Issue changes that can overwrite coordination state require the current lease
and perform validation plus mutation under one SQLite write transaction. New
Issues and additive comments remain available without a lease so work can be
captured and progress can be appended. The exact protection boundary is in
[`design/issue-leases.md`](design/issue-leases.md).

Issue and Project state names are configurable global data. Their small type
axes carry lifecycle meaning; names carry workflow-specific meaning. See
[`design/configurable-states.md`](design/configurable-states.md).

## Read surfaces

The CLI's text and JSON commands use the `Store` facade and domain projections.
The TUI also consumes `IssueDetail` through `Store::list_all_issue_details`; it
does not maintain a second Issue query model and has no mutation commands.

`src/query` is a separate read path because GraphQL selection changes the data
needed for each request. It owns a read-only schema and compiles each selected
root into a selection-aware SQLite statement. It shares the database and scope
with `Store`, but not the ordinary app/store projection path. The subsystem is
described in [`query.md`](query.md), with its exact compilation contract in
[`design/selection-aware-queries.md`](design/selection-aware-queries.md).

## Public and retained capabilities

The 0.1.0 public surface is Issue-centered: Issues, configuration, Projects,
Milestones, repositories, GraphQL reads, and the Issue TUI. Pull Request and
Wiki domain, application, store, SQL, and schema code remain in the repository
for staged future support, but no top-level CLI command or public GraphQL field
exposes them. Retained implementation is not permission to accidentally publish
those surfaces; promotion requires its own product decision and tests.

The skill under `skills/octa` describes how agents use the released CLI. It is
a consumer of product behavior, not an architectural specification for this
repository.

## Where recurring changes go

- A new persisted field usually crosses `migrations/0001_init.sql`, the owning
  `sql` adapter, domain projections, app/store facades, each public output that
  promises the field, integration tests, and SQLx offline metadata.
- A new invariant belongs as low as it can be enforced without losing its
  meaning: schema constraints for representability, an SQL transaction for
  atomicity, and `app` for rules requiring workflow context.
- A new read-only GraphQL field starts in `query/model.rs`, gains a projection
  in `query/sql.rs`, and is exposed from `query/root.rs`; planner tests must show
  that unselected tables are absent and repository correlation remains intact.
- A new interactive display may consume domain projections, but it must not
  fork persistence policy or create a second source of Issue semantics.

Schema changes also follow the pre-release database replacement contract in
[`design/schema-evolution.md`](design/schema-evolution.md).
