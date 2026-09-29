# Read-only Query System

`octa query` is a read-only GraphQL surface over the same SQLite store used by
the CLI. It exists for consumers that need a shaped, machine-readable graph of
Issues, Projects, Milestones, labels, and repositories without turning the
GraphQL runtime into a second mutation API.

## One active repository

The query schema is constructed from an already-open `Store` and requires one
active repository. Entity roots and nested relations compile against that
repository. `repositories` is the deliberate exception: it lists scalar
metadata for every registered repository, but exposes no relation that could
descend into several repository graphs in one request.

This keeps repository scope a construction-time fact rather than a field that
each resolver may forget to apply. Global label configuration is correlated to
the active repository when traversing back to Issues or Projects.

## Selection-aware execution

The GraphQL field tree is not resolved by loading full domain objects and then
visiting their relations. `query/sql.rs` compiles the requested root selection
into a SQLite JSON projection:

```text
GraphQL selection
      |
      v
merge aliases and fragments
      |
      v
compile selected scalars, filters, and correlated relation subqueries
      |
      v
one SQL statement per selected root field
      |
      v
JSON object rows --> lightweight GraphQL objects
```

Unselected fields contribute no columns, joins, or relation subqueries. Nested
collections apply their own stable ordering and pagination inside the
correlated subquery. GraphQL aliases become JSON response keys, and repeated
selection paths from fragments are merged so the relation is projected once.

The exact planner rules and extension procedure are in
[`design/selection-aware-queries.md`](design/selection-aware-queries.md).

## Safety bounds

The schema contains `QueryRoot`, `EmptyMutation`, and `EmptySubscription`.
Requests are limited to depth 8 and complexity 500. Collections default to 50
items and accept limits from 1 through 100 with a non-negative offset. The
response extension `dbAccesses` reports the number of root SQL executions,
which makes accidental resolver-driven query growth observable.

Pull Request and Wiki types are absent from both the public schema and runtime
selection compiler even though their internal storage is retained. Schema SDL
from `octa query --schema` is the version-specific public reference.

## Verification boundary

`query/sql_tests.rs` tests the generated SQL rather than only the final JSON.
It fixes the properties that matter to the architecture: selected-only joins,
repository predicates, direction of Issue relations, correlation across
multiple hops, per-collection pagination, alias/fragment merging, and rejection
of withdrawn public types. CLI integration tests cover schema output and
execution against migrated SQLite data.
