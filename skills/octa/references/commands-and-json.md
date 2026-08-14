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

The selection set is compiled into a SQLite query that projects only selected columns and relations. Singular relations use correlated joins; collection relations use aggregate subqueries containing joins. Unselected relations are not queried. The command writes a standard GraphQL JSON response to stdout, with the executed query count in `extensions.dbAccesses`. Successful responses contain `data`; validation and execution failures contain `errors` in the envelope and may still exit successfully. Consumers must inspect the envelope rather than relying only on process status. Use `query --schema` or GraphQL introspection to inspect the available public fields rather than depending on SQLite tables.

## Issue

```text
issue create|list|show|set|unset|add|remove|comment|set-state|lock|unlock|tui
```

Important constraints:

- `issue lock N` returns a random opaque non-expiring lease ID once. A second acquisition fails while the Issue is leased.
- `issue set-state`, `set`, `unset`, `add`, and `remove` require the target Issue's `--lease`. So do Issue–PR link changes through `pr create --issue`, `pr add`, and `pr remove`.
- Normal `issue unlock` requires the matching `--lease`. `issue unlock --force` accepts no lease and invalidates the old credential immediately; use it only for recovery.
- `issue create`, `issue comment`, unlinked `pr create`, `pr comment`, `pr set`, `pr set-state`, and Project, Milestone, Wiki, and config operations do not require an Issue lease. Read commands and lease acquisition also require no existing lease credential.
- Priority is `0` (none), `1` (urgent), `2` (high), `3` (medium), or `4` (low).
- Status types are `backlog`, `unstarted`, `started`, `completed`, and `canceled`.
- With no state selector or with `--open`, `issue list` returns non-terminal Issues. `--closed` returns terminal Issues, `--all` returns both, and `--state <name>` exactly matches a configured state name. These four selectors are mutually exclusive.
- A Milestone belongs to a Project. `issue create --milestone` requires `--project`.
- Unset an Issue's Milestone before changing or unsetting its Project.
- Parent/child relations are repository-local and independent of Project membership. Parent and child may belong to different Projects, or only one may have a Project.
- Setting a parent initially inherits the parent's Project when the child has none. Later Project changes or removal do not propagate across the relation and are not blocked by it.
- `issue list --related-to N` filters symmetric relations; `--unblocked` excludes Issues with non-terminal blockers.
- `issue tui` is an interactive read-only browser. For agent automation, prefer `issue list --json` and `issue show --json`.

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
config state create|set|delete|set-default|list
```

Wiki bodies recognize `[[slug]]`; `wiki show` includes outgoing links and backlinks. Issue and Project labels have separate definitions selected by the required `--target`. Label and label-group names are opaque repository data with no reserved operational taxonomy. Label groups use `single` or `multi` selection; `single` only makes explicitly grouped labels for the same target mutually exclusive. `config state create --type` accepts the five status types above, with optional `--starting` and `--terminal`. `config state set <name>` changes a state's `--name`, `--type`, or `--terminal`; renaming carries the state's Issues with it. `config state delete <name>` needs `--move-to <state>` while Issues still reference the state, and refuses the starting state. `config state set-default <name>` moves the starting flag.

New repositories seed Backlog, Todo, In Progress, In Review, Done, and Canceled — Linear's default workflow minus its `duplicate` status type, which octa does not model. Seeding happens only for a repository with no configured states, so an existing workflow is never extended behind your back. Exactly one state carries the starting flag and receives new Issues; it is resolved from that flag alone. Issue states carry no stored ordinal — `config state list` derives its order from `status_type` (backlog, unstarted, started, completed, canceled) and then name, so there is nothing to reorder. Existing custom and legacy states and their Issues are preserved.

## JSON contracts

Commands exposing `--json` write one JSON value to stdout:

- List commands return arrays.
- `issue show` returns the Issue fields plus `labels`, `blocks`, `blocked_by`, `related`, `pull_requests`, `parent`, `sub_issues`, and `comments`.
- An Issue includes `repo`, `number`, `title`, `body`, `state`, `status_type`, `priority`, optional `project`, optional `milestone`, `leased`, `created_at`, and `updated_at`. The lease ID is never exposed by list, show, or query output.
- `project list` includes every Project by default, including completed and canceled Projects. Use `--active` for the explicit non-terminal Project filter.
- Project list/show tallies count all assigned Issues and expose backlog, unstarted, started, completed, canceled, and total; canceled work is never subtracted implicitly.
- Projects are ordered by priority 1 through 4, then priority 0 (None).
- `project show` also includes `issue_numbers`, `labels`, and the full tally.
- `pr show` includes PR fields and `comments`.
- `wiki show` includes page fields, `links_to`, and `backlinks`.

Do not depend on human-readable column spacing. When exact future fields matter, run the command and inspect the returned JSON because additive fields may appear.
