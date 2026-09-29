# Selection-aware GraphQL Queries

How does octa expose nested GraphQL reads without loading complete records or
issuing one database query per object and relation?

## The rule

Each selected root field compiles to one SQL statement whose projection mirrors
that root's GraphQL selection. SQLite constructs JSON objects and arrays;
`query/model.rs` exposes lightweight GraphQL objects that read already-projected
values rather than returning to the database from field resolvers.

The planner walks merged `SelectionField` values and emits only requested work:

- scalar fields select their owning columns;
- optional joins appear only when a selected field or filter requires them;
- singular relations are correlated scalar subqueries;
- collection relations are correlated, ordered, paginated subqueries aggregated
  into JSON arrays;
- filters use scoped predicates or `EXISTS` subqueries instead of widening the
  projection;
- aliases become JSON keys, and fragment selections with the same response key
  merge before compilation;
- `__typename` is ignored by the SQL projection.

Every generated alias is unique within the root statement. Every entity
correlation carries `repository_id`; global labels use the planner's active
repository when traversing back to repository-scoped entities.

## Paging and ordering

Root and nested collections independently accept `offset` and `limit`. Offset
must be non-negative; limit defaults to 50 and must be between 1 and 100.
Ordering is part of the relation contract and occurs inside its subquery before
aggregation: Issues by number, Projects by ID, Milestones by position then ID,
labels by name, and repositories by name then ID.

## Schema boundary

`QueryRoot` is paired with `EmptyMutation` and `EmptySubscription`. Most roots
require the active repository captured from `Store`. `repositories` is the one
store-wide root and remains scalar-only; allowing it to descend into entity
relations would violate the single-active-repository premise used by every
other compiled root.

The schema has depth and complexity limits in addition to per-collection
pagination. `dbAccesses` counts root executions, making a query with multiple
root fields explicit while preserving one statement for each root.

## Extending the schema

To add a field:

1. define its GraphQL accessor or input in `query/model.rs`;
2. teach the matching `Planner` method in `query/sql.rs` to project it;
3. add or extend the root in `query/root.rs` when the field is not nested;
4. test the generated SQL for positive evidence and for the absence of
   unrelated joins or subqueries;
5. test aliases/fragments, pagination, repository correlation, and each
   relationship direction that applies;
6. update public documentation only after `octa query --schema` exposes the
   intended contract.

Never add a resolver that performs per-object database access merely because
async-graphql makes that implementation convenient. That changes the execution
model from a bounded projection into query growth proportional to the result
graph.

## Why compile to JSON in SQLite?

Loading complete domain projections would fetch data the client did not ask for
and would still need a strategy for nested relations. Resolver-driven loading
is simpler locally but creates N+1 behavior and makes access count depend on
result size. A selection-aware JSON projection keeps requested shape, relation
ordering, pagination, and database work visible in one inspectable statement.
