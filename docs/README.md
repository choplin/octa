# Documentation Policy

Everything under `docs/` is for developers changing octa. User-facing setup and
command guidance belong in the top-level [`README`](../README.md), while agent
workflow guidance belongs under [`skills/`](../skills/).

This file defines how the documentation is organized. It deliberately does not
list the documents; adding or removing a topic must not require updating this
policy.

## Top level: the mental model

[`architecture.md`](architecture.md) is the entry point. Other top-level files
each explain one coherent architectural unit: its purpose, owners, relationships,
and governing constraints. They are compressed mental models, not file catalogs
or complete API references, and every one must be reachable from
`architecture.md`.

Put a detail at the top level when a developer needs it to predict where a
behavior belongs or how an end-to-end flow crosses boundaries. Put exact rules,
edge cases, procedures, rationale, and rejected alternatives under `design/`.

## `design/`: one design question per file

Each file under `design/` owns one question that could reasonably have been
answered another way. It states the current rule precisely, explains why octa
uses it, records rejected alternatives when they still clarify the boundary,
and gives implementers the examples or verification obligations they need.

These files are not ADRs. When a rule changes, rewrite its design document to
describe the new current state. Git retains the old text; the decision log
retains the chronology that remains useful.

## `decision-log.md`: chronology

[`decision-log.md`](decision-log.md) is a concise, newest-first record of when a
binding design choice was introduced or replaced and why. Each entry links to
the current document that owns the resulting rule. Procedures and
implementation detail do not belong in the log.

## Single source of truth

Give every settled claim one canonical home. An architectural overview may
summarize a design rule at the depth needed for the mental model, then link to
the design document for its precise contract and rationale. Work in progress,
proposals, and migration status belong in the tracker rather than here.
