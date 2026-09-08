# Commands and JSON

Use `octa <surface> --help` and nested help as the executable source of truth. The summary below identifies the current product surface without prescribing a work process.

Examples assume an installed `octa` executable. When developing octa itself, build it and use `./target/debug/octa`; install from the source tree with `cargo install --path .` only when installation is explicitly requested.

## Read-only GraphQL query

```text
query [--file <path>] [--variables <json-object>]
query --schema
```

Without `--file`, the GraphQL document is read from stdin. The schema exposes Issue, Project, Milestone, Repository, and Issue/Project label fields and relations. Entity fields other than the repository listing use the selected repository. It has no mutation type. List fields use `offset` and `limit`; the default limit is 50 and the maximum is 100. Query depth is limited to 8 and complexity to 500.

For example, stdin, variables, and a nested selection can be used together:

```sh
octa query --variables '{"number": 1}' <<'GRAPHQL'
query IssueContext($number: Int!) {
  issue(number: $number) {
    number
    project { name }
    labels { name }
  }
}
GRAPHQL
```

A file uses the same execution path:

```sh
octa query --file issue.graphql --variables '{"number": 1}'
```

An Issue exposes `stateType` alongside `state`, and `IssueFilter` takes `state`
and `stateType`. Both filter entries accept one value or a list, and match an
Issue carrying any listed value:

```sh
octa query <<'GRAPHQL'
{
  issues(filter: { stateType: ["open", "in progress"] }) {
    number
    state
    stateType
  }
}
GRAPHQL
```

The selection set is compiled into a SQLite query that projects only selected columns and relations. Singular relations use correlated joins; collection relations use aggregate subqueries containing joins. Unselected relations are not queried. The command writes a standard GraphQL JSON response to stdout, with the executed query count in `extensions.dbAccesses`. Successful responses contain `data`; validation and execution failures contain `errors` in the envelope and may still exit successfully. Consumers must inspect the envelope rather than relying only on process status. Use `query --schema` or GraphQL introspection to inspect the available public fields rather than depending on SQLite tables.

## Issue

```text
issue open|create|list|show|start|close|reopen|set|unset|add|remove|lock|unlock|tui
issue comment add|show|delete
```

Important constraints:

- `issue lock N` returns a random opaque non-expiring lease ID once. A second acquisition fails while the Issue is leased.
- `issue start`, `close`, `reopen`, `set`, `unset`, `add`, `remove`, and `comment delete` require the target Issue's `--lease`.
- Normal `issue unlock` requires the matching `--lease`. `issue unlock --force` accepts no lease and invalidates the old ID immediately; use it only for recovery.
- `issue open` (alias `create`), `issue comment add`, and Project, Milestone, Repository, and config operations do not require an Issue lease. Read commands, including `issue comment show`, and lease acquisition also require no existing lease ID.
- `issue open`, `issue set`, and `issue comment add` accept either `--body <text>` or `--body-file <path>`; pass `--body-file -` to read from stdin. The two input forms are mutually exclusive.
- Each verb without an explicit target moves the Issue to its type's default state: `open` to the `open` default, `start` to the `in progress` default, `close` to the `closed` default, `reopen` back to the `open` default. `--as <state>` picks another state of that verb's own type and rejects any other type, `issue open` included. `start` has no `--as`. `issue set --as <state>` is the only unconstrained move and reaches any configured state; capturing work that is already underway is `open` then `start`.
- With no state selector, `issue list` returns Issues outside the `closed` type. `--state <names>` matches configured state names, `--state-type <types>` matches state types, both comma-separated and matching any listed value, and `--all` applies no filter. These three selectors are mutually exclusive. States are global configuration, so `--state` and `--state-type` also work under `--all-repositories`.
- A Milestone belongs to a Project. `issue open --milestone` requires `--project`.
- Unset an Issue's Milestone before changing or unsetting its Project.
- Parent/child relations are repository-local and independent of Project membership. Parent and child may belong to different Projects, or only one may have a Project.
- Setting a parent initially inherits the parent's Project when the child has none. Later Project changes or removal do not propagate across the relation and are not blocked by it.
- `issue list --related-to N` filters symmetric relations; `--unblocked` excludes Issues with open blockers.
- `issue tui` is an interactive read-only browser. For agent automation, prefer `issue list --json` and `issue show --json`.

## Project and Milestone

```text
project create|list|show|set|add|remove|close|reopen
milestone create|list|show|set|unset --project <project>
```

Projects and Milestones accept a numeric id or name where shown by help. `project create --as`, `project close --as`, and `project reopen --as` accept a state of the verb's own type; `project set --as` can move to any configured Project state. Milestones have a Project-local name, position, status, optional description, and optional start/target dates.

## Repository

```text
repository list|register|set|relocate
```

Registration is normally implicit. Use `repository register --name <name> [path]` to choose a unique identity explicitly, `repository set <name> --name <new-name>` to rename it, and `repository relocate <name> [path]` after moving an already-registered Git repository. The latter two operations coordinate the database identity with repository-local Git configuration. The former `repo` spelling remains a compatibility alias.

## Label and State Configuration

```text
config issue state create|set|delete|list
config issue label create|list
config issue label-group create|list
config project state create|set|delete|list
config project label create|list
config project label-group create|list
```

Issue and Project configuration is global: one set of states, labels, and label groups governs every repository. Issue and Project label definitions are separate, selected by the `issue` or `project` segment of the command path. Label and label-group names are opaque data with no reserved operational taxonomy. Label groups use `single` or `multi` selection; `single` only makes explicitly grouped labels for the same target and group mutually exclusive.

An Issue state has one immutable type: `open`, `in progress`, or `closed`. `config issue state create <name>` takes `--type` (default `open`) and `--default`. `config issue state set <name>` can rename the state or make it its type's default; renaming carries the state's Issues with it. `config issue state delete <name>` needs `--move-to <state>` while Issues still reference it. A state cannot be retyped after creation.

A Project state follows the same create/set/delete model with the types `open` and `closed`. Issue and Project states are separate definitions. `project create` and `project reopen` use the default open Project state, while `project close` uses the default closed Project state unless `--as` selects another state of the required type.

New stores seed Issue states `open`, `in progress`, `closed`, and `not planned`, and Project states `open`, `closed`, and `not planned`. Seeding happens only when the corresponding global state set is empty, so an existing workflow is not extended automatically.

A populated type has exactly one default. The first state created in an empty type becomes its default automatically. The open and closed types cannot be emptied; the Issue `in progress` type may be empty, in which case `issue start` has no destination and fails. A default cannot be deleted while another state of that type remains; make another state the default first. State lists derive their order from type, default status, and name rather than a stored ordinal.

## JSON contracts

Commands exposing `--json` write one JSON value to stdout:

- List commands return arrays.
- `issue show` returns the Issue fields plus `labels`, `blocks`, `blocked_by`, `related`, `pull_requests`, `parent`, `sub_issues`, and `comments`.
- `issue list` returns each Issue with a `labels` array in addition to its scalar and optional relationship fields.
- `issue comment show <issue> <comment-id> --json` returns one comment with `id`, `body`, and `created_at`.
- An Issue includes `repo`, `number`, `title`, `body`, `state`, optional `project`, optional `milestone`, `leased`, `created_at`, and `updated_at`. `config issue state list --json` returns each state's `name`, `type`, and `is_default`. The lease ID is never exposed by list, show, or query output.
- `project list` includes every Project by default, including closed ones. Use `--active` for the explicit open-Project filter. A Project includes `state` and `state_type`; whether it is closed is derived from `state_type`.
- Project list/show tallies count all assigned Issues and expose open, closed, and total; closed work is never subtracted implicitly.
- Projects are ordered by repository name, then by Project id. octa has no built-in priority; express one with a `single` label group when needed.
- `project show` also includes `issue_numbers`, `labels`, and the full tally.
- `repository list --json` returns `name`, `path`, `created_at`, `updated_at`, `open_issues`, and `in_progress_issues` for each repository. Repository register, set, and relocate with `--json` return the updated repository object.

Do not depend on human-readable column spacing. When exact future fields matter, run the command and inspect the returned JSON because additive fields may appear.
