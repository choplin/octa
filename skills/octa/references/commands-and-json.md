# Commands and JSON

Use `octa <surface> --help` and nested help as the executable source of truth. The summary below identifies the current product surface without prescribing a work process.

## Issue

```text
issue create|list|show|set|unset|add|remove|comment|set-state|lock|unlock
```

Important constraints:

- Priority is `0` (none), `1` (urgent), `2` (high), `3` (medium), or `4` (low).
- Status types are `backlog`, `unstarted`, `started`, `completed`, and `canceled`.
- A Milestone belongs to a Project. `issue create --milestone` requires `--project`.
- Unset an Issue's Milestone before changing or unsetting its Project.
- Parent/child relations are repository-local and independent of Project membership. Parent and child may belong to different Projects, or only one may have a Project.
- Setting a parent initially inherits the parent's Project when the child has none. Later Project changes or removal do not propagate across the relation and are not blocked by it.
- `issue list --related-to N` filters symmetric relations; `--unblocked` excludes Issues with non-terminal blockers.

## Project and Milestone

```text
project create|list|show|set|add|remove|set-state
milestone create|list|show|set|unset --project <project>
```

Projects and Milestones accept a numeric id or name where shown by help. Milestones have a Project-local name, position, status, optional description, and optional start/target dates.

## Pull Request

```text
pr create|list|show|set|comment|set-state|add|remove
```

octa records PR metadata and discussion; Git remains the source for code and diffs. Issue-to-PR links are explicit and many-to-many within a repository; identical pairs are stored once.

## Wiki, Label, and State

```text
wiki create|set|show|list
config label create|list --target issue|project
config label-group create|list --target issue|project
config state create|list
```

Wiki bodies recognize `[[slug]]`; `wiki show` includes outgoing links and backlinks. Issue and Project labels have separate definitions selected by the required `--target`. Label and label-group names are opaque repository data with no reserved operational taxonomy. Label groups use `single` or `multi` selection; `single` only makes explicitly grouped labels for the same target mutually exclusive. `config state create --type` accepts the five status types above, with optional `--starting` and `--terminal`.

New repositories seed only the compatibility states `open`, `in_progress`, and `closed`. Workflow names such as Backlog, Todo, In Progress, In Review, Done, and Canceled are repository-owned custom states; create whichever ones are useful with `config state create --type`. Existing custom and legacy states and their Issues are preserved.

## JSON contracts

Commands exposing `--json` write one JSON value to stdout:

- List commands return arrays.
- `issue show` returns the Issue fields plus `labels`, `blocks`, `blocked_by`, `related`, `pull_requests`, `parent`, `sub_issues`, and `comments`.
- An Issue includes `repo`, `number`, `title`, `body`, `state`, `status_type`, `priority`, optional `project`, optional `milestone`, optional `locked_by`, `created_at`, and `updated_at`.
- `project list` includes every Project by default, including completed and canceled Projects. Use `--active` for the explicit non-terminal Project filter.
- Project list/show tallies count all assigned Issues and expose backlog, unstarted, started, completed, canceled, and total; canceled work is never subtracted implicitly.
- Projects are ordered by priority 1 through 4, then priority 0 (None).
- `project show` also includes `issue_numbers`, `labels`, and the full tally.
- `pr show` includes PR fields and `comments`.
- `wiki show` includes page fields, `links_to`, and `backlinks`.

Do not depend on human-readable column spacing. When exact future fields matter, run the command and inspect the returned JSON because additive fields may appear.
