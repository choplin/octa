# Decision Log

Newest first. Each entry records the decision and its reason, then links to the
document that owns the current rule.

| Date | Decision and reason | Current rule |
| --- | --- | --- |
| 2026-09-08 | Narrowed the 0.1.0 public surface to the exercised Issue workflow while retaining Pull Request and Wiki storage internally, so the first release does not promise undogfooded workflows. | [`architecture.md`](architecture.md) |
| 2026-09-06 | Made repository identity relocatable by separating a stable Git-local name from the current canonical path, so moving a checkout does not create a new collaboration history. | [`design/repository-identity-and-scope.md`](design/repository-identity-and-scope.md) |
| 2026-08-18 | Replaced independent state flags with a closed type axis and per-type defaults, so lifecycle verbs and filters depend on one non-contradictory classification while names remain configurable. | [`design/configurable-states.md`](design/configurable-states.md) |
| 2026-08-17 | Made Issue and Project configuration global rather than repository-scoped, so one local octa store has one workflow vocabulary and repository selectors cannot misleadingly target configuration. | [`persistence.md`](persistence.md), [`design/configurable-states.md`](design/configurable-states.md) |
| 2026-08-14 | Added selection-aware, read-only GraphQL queries that compile each selected root to one SQLite JSON projection, avoiding full-object loading and resolver-driven query growth. | [`query.md`](query.md), [`design/selection-aware-queries.md`](design/selection-aware-queries.md) |
| 2026-08-13 | Replaced actor-named locks with opaque lease capabilities, so ownership does not require a user model and protected mutation can validate possession atomically. | [`design/issue-leases.md`](design/issue-leases.md) |
| 2026-07-25 | Kept pre-release schema history as one rewritten initial migration, so fresh installations receive the final schema rather than abandoned intermediate designs. | [`design/schema-evolution.md`](design/schema-evolution.md) |
| 2026-07-23 | Separated CLI transport, workflow policy, domain projections, and SQL adapters, so presentation does not own business rules and database row shapes do not escape persistence. | [`architecture.md`](architecture.md) |
| 2026-07-22 | Chose one user-global SQLite database with repository-scoped entity keys, giving linked worktrees shared data and allowing cross-repository views without external hosting. | [`persistence.md`](persistence.md) |
