# Configurable States

How does octa allow workflow-specific state names while retaining stable
lifecycle semantics for commands, filters, and aggregates?

## The rule

State names are configurable global data. A separate closed type axis supplies
the lifecycle meaning that code may depend on:

- Issue states: `open`, `in progress`, or `closed`;
- Project states: `open` or `closed`.

Names such as `Todo`, `Review`, or `Canceled` express workflow meaning. Code
must not infer lifecycle behavior from those names. An Issue counts as resolved
only when its state's type is `closed`; a Project is active when its state's
type is not `closed`.

Each populated type has exactly one default state. Named lifecycle verbs resolve
that default when the caller gives no explicit target: Issue open/reopen uses
the `open` default, start uses `in progress`, and close uses `closed`; Project
create/reopen and close use their corresponding defaults.

## Database representation

Defaults live in separate tables keyed by type rather than as flags on state
rows. This gives each type at most one default and lets replacing a default be
one row update. A composite foreign key `(name, type)` prevents a default from
pointing at a state of another type.

A state's type is immutable. Retyping would silently change the lifecycle
meaning of every referencing entity and could leave the old type without a
default. Callers instead create the intended state and delete the old state
while explicitly naming where existing entities move.

Deleting the final state of a type may remove its default. Deleting one state
while others of that type remain must preserve or replace the default in the
same operation. Renaming a state cascades to entities and its default row;
deleting a state still referenced by an entity is restricted.

## Seeding and help

`Store::open` seeds the built-in state sets only when no states of that entity
kind exist. A customized store keeps exactly its configured set. Help output
opens only an existing database and injects current configured values without
creating a store; parser validation and error messages derive from the same
closed type constants or loaded names so advertised and accepted values stay
aligned.

## Rejected alternatives

- Fixed state names would make octa own each user's workflow.
- Name conventions or labels would leave lifecycle filters and verbs dependent
  on informal spelling.
- Independent `starting` and `terminal` flags admit contradictory combinations;
  one type axis represents the three Issue lifecycle classes directly.
- Giving Projects an `in progress` type would duplicate information already
  visible from their Issue tally. Project names may still express that nuance.

## Verification

Schema and application tests must cover default replacement, rename cascades,
deletion with and without a move target, immutable types, a populated type
retaining one default, seed idempotence, and help/parser agreement.
