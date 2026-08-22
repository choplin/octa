# Commands and JSON

Use `octa <surface> --help` and nested help as the executable source of truth. The summary below identifies the current product surface without prescribing a work process.

Examples assume an installed `octa` executable. When developing octa itself, build it and use `./target/debug/octa`; install from the source tree with `cargo install --path .` only when installation is explicitly requested.

## Read-only GraphQL query

```text
query [--file <path>] [--variables <json-object>]
query --schema
```

Without `--file`, the GraphQL document is read from stdin. The schema exposes Issue, Project, Milestone, Pull Request, Wiki, and Issue/Project label fields and relations for the selected repository. It has no mutation type. List fields use `offset` and `limit`; the default limit is 50 and the maximum is 100. Query depth is limited to 8 and complexity to 500.

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
issue open|create|list|show|start|close|reopen|set|unset|add|remove|comment|lock|unlock|tui
```

Important constraints:

- `issue lock N` returns a random opaque non-expiring lease ID once. A second acquisition fails while the Issue is leased.
- `issue start`, `close`, `reopen`, `set`, `unset`, `add`, and `remove` require the target Issue's `--lease`. So do Issue–Pull Request link changes through `pull-request create --issue`, `pull-request add`, and `pull-request remove`.
- Normal `issue unlock` requires the matching `--lease`. `issue unlock --force` accepts no lease and invalidates the old credential immediately; use it only for recovery.
- `issue open` (alias `create`), `issue comment`, unlinked `pull-request create`, `pull-request comment`, `pull-request set`, `pull-request set-state`, and Project, Milestone, Wiki, and config operations do not require an Issue lease. Read commands and lease acquisition also require no existing lease credential.
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
project create|list|show|set|add|remove|set-state
milestone create|list|show|set|unset --project <project>
```

Projects and Milestones accept a numeric id or name where shown by help. Milestones have a Project-local name, position, status, optional description, and optional start/target dates.

## Pull Request

```text
pull-request create|list|show|set|comment|set-state|add|remove
```

octa records Pull Request metadata and discussion; Git remains the source for code and diffs. Issue-to-Pull Request links are explicit and many-to-many within a repository; identical pairs are stored once. The former `pr` command remains a compatibility alias.

## Wiki, Label, and State

```text
wiki create|set|show|list
config issue state create|set|delete|list
config issue label create|list
config issue label-group create|list
config project label create|list
config project label-group create|list
```

Wiki bodies recognize `[[slug]]`; `wiki show` includes outgoing links and backlinks. Issue and Project labels have separate definitions, selected by the `issue` or `project` segment of the command path. Label and label-group names are opaque repository data with no reserved operational taxonomy. Label groups use `single` or `multi` selection; `single` only makes explicitly grouped labels for the same target mutually exclusive. `config issue state create <name>` takes `--type open|in progress|closed` (default `open`) and `--default`, which makes it that type's default. `config issue state set <name>` changes a state's `--name` or `--type`, and `--default` makes it its own type's default; renaming carries the state's Issues with it. The default needs no type argument anywhere, since a state already carries exactly one type. `config issue state delete <name>` needs `--move-to <state>` while Issues still reference the state; the schema refuses the delete otherwise.

New repositories seed `open`, `in progress`, `closed`, and `not planned` — one state per type plus the second way work ends. Seeding happens only for a repository with no configured states, so an existing workflow is never extended behind your back. A workflow that separates capture from grooming, or execution from review, configures those states itself.

A state carries exactly one classification: its `type`, one of `open`, `in progress`, or `closed`. octa models no finer gradation. An Issue is closed when its state's type is `closed`; that is derived from the type, not a second stored flag. Why an Issue closed is a reason carried by the state name, which is why `closed` and `not planned` share one type instead of the axis gaining a fourth value.

An Issue's state is a reference to a configured state, enforced by the schema: an Issue cannot name a state that does not exist, renaming a state carries its Issues with it, and a state that still holds Issues cannot be deleted. Deleting a state never deletes Issues.

A type that has any states has exactly one default, the one a verb resolves to when given no explicit target. The schema holds that invariant rather than the application: the default is one row per type, keyed by the type, referencing a state of that same type. The first state of an empty type becomes that default whether or not `--default` was passed — by creation or by retyping an existing state into it — and the command says so. Only an empty type has no default, which is why emptying `in progress` is legal and emptying `open` or `closed` is not. The `open` and `closed` types must stay populated, since every Issue has to be able to start and to end; the `in progress` type may be emptied, and then `start` reports that it has nowhere to go. A type's default cannot be deleted or retyped while another state of that type remains — move the default first. Issue states carry no stored ordinal: `config issue state list` derives its order from type (`open`, then `in progress`, then `closed`), then the type's default, then name. Existing custom states and their Issues are preserved.

## JSON contracts

Commands exposing `--json` write one JSON value to stdout:

- List commands return arrays.
- `issue show` returns the Issue fields plus `labels`, `blocks`, `blocked_by`, `related`, `pull_requests`, `parent`, `sub_issues`, and `comments`.
- An Issue includes `repo`, `number`, `title`, `body`, `state`, optional `project`, optional `milestone`, `leased`, `created_at`, and `updated_at`. `config issue state list --json` returns each state's `name`, `type`, and `is_default`. The lease ID is never exposed by list, show, or query output.
- `project list` includes every Project by default, including closed ones. Use `--active` for the explicit open-Project filter. A Project's closed flag is set by `project create --closed` and `project set-state <project> <state> --closed`.
- Project list/show tallies count all assigned Issues and expose open, closed, and total; closed work is never subtracted implicitly.
- Projects are ordered by repository name, then by Project id. octa has no built-in priority; express one with a `single` label group when needed.
- `project show` also includes `issue_numbers`, `labels`, and the full tally.
- `pull-request show` includes Pull Request fields and `comments`.
- `wiki show` includes page fields, `links_to`, and `backlinks`.

Do not depend on human-readable column spacing. When exact future fields matter, run the command and inspect the returned JSON because additive fields may appear.
