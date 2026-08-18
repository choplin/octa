# octa

**A CLI that brings team-grade collaboration to solo, AI-agent-driven development, locally.**

octa is a local collaboration substrate for individual developers who run several AI agents and several development sessions in parallel on a single machine.

With a GitHub-like mental model of Issues, Pull Requests, and a Wiki, it keeps what a repository is aiming at and what is left to do across sessions.

Data is never sent to an external service; it is stored in a local SQLite database.

## What octa solves

When work is split across AI agents, the reasoning behind decisions, the unfinished work, and the context to hand to whoever comes next get scattered across sessions.

octa turns that information into a record that persists per repository.

- **Issue**: a work record with a state, dependencies, comments, labels, and an atomic lease.
- **Pull Request**: a record of discussion and state tied to a Git branch. Code and diffs stay on the Git side.
- **Wiki**: pages for policies and procedures, with `[[slug]]` links and backlinks.
- **Labels and states**: the classification and workflow each project configures for itself.

octa does not provide Git hosting, a web UI, remote sync, authentication, or real-time collaboration for many people.

It is focused on coordinating the multiple worktrees, agents, and sessions running on the same machine.

## Prerequisites

- Run it inside a Git repository.
- Rust 1.89 or later and Cargo.

This repository also ships a Nix development environment.

```sh
nix develop
cargo build
```

To install it locally and use it as the `octa` command:

```sh
cargo install --path .
```

`~/.cargo/bin` must be on your `PATH`.

To use the development binary without installing:

```sh
./target/debug/octa --help
```

The examples below assume `octa` is on your `PATH`.

## The first five minutes

First, move into the target Git repository.

```sh
cd path/to/your-repository
```

Create an Issue, then list and inspect it.

```sh
octa issue open \
  --title "Document the release process" \
  --body "Record the required checks and the steps in the Wiki."

octa issue list
octa issue show 1
```

An agent or session that starts working can take an exclusive **lease** with no expiry.
`issue lock` prints a human-friendly three-word lease ID such as `amber-otter-lantern`
to standard output exactly once. A lease ID is not a security credential; it is an
ownership ID that prevents accidental concurrent edits. Keep it so later commands
can reuse it.

```sh
LEASE=$(octa issue lock 1)
octa issue start 1 --lease "$LEASE"
octa issue comment 1 --body "Started working on this."
```

When the work is done, close the Issue with the same lease, then release the lease.

```sh
octa issue close 1 --lease "$LEASE"
octa issue unlock 1 --lease "$LEASE"
```

If you lose the lease ID, `--force` releases it as a recovery operation.
The previous lease ID becomes invalid immediately, and resuming work requires taking a new lease.

```sh
octa issue unlock 1 --force
LEASE=$(octa issue lock 1)
```

`issue start`, `close`, `reopen`, `set`, `unset`, `add`, `remove`, a regular `unlock`, and the commands that change the link between an Issue and a PR — `pr create --issue`, `pr add`, `pr remove` — all require the target Issue's `--lease`.
Creating and commenting on Issues, commenting on PRs, creating a PR that links to no Issue, `set` / `set-state` on the PR itself, and Project, Milestone, Wiki, and config operations need no lease.
Neither do read operations.
Lease IDs appearing in tool logs and command arguments is expected. Do not put them in
durable records such as Issue comments, Git artifacts, or repository files. `issue list`
and `issue show` do not display lease IDs; they only report whether one is held, via `leased`.

## Coordinating work with Issues

An Issue has a number, a body, comments, a state, dependencies, and labels.

The examples below that modify the existing Issue 1 use the lease taken earlier.

```sh
LEASE=$(octa issue lock 1)
```

### States and listing

A new repository gets four states — `open`, `in progress`, `closed`, and `not planned` —
and new Issues land in `open`. Every state carries exactly one **type** out of the three
values `open` / `in progress` / `closed`.

```sh
octa issue start 1 --lease "$LEASE"
octa issue list
octa issue list --state-type "in progress"
octa issue list --state-type closed
octa issue list --state "open,not planned"
octa issue list --all
```

With no selector, the listing excludes Issues in closed-type states. `--state-type <types>`
matches on the type and `--state <names>` matches exactly on configured state names; both
accept a `,`-separated list and return Issues matching any of them. `--all` applies no
filtering. These three selectors are mutually exclusive: naming a state already fixes its
type, so combining `--state` with `--state-type` is always either redundant or always empty.

A read-only two-pane TUI is available as an alternative to listing and inspecting Issues.
This describes the current initial implementation; it does not exclude future TUI mutation
operations from the product boundary.
The default `filter: all` shows every Issue in the current repository — not just candidates
for work, but also closed-type Issues and existing custom states — ordered by Issue number.

```sh
octa issue tui
```

Select an Issue with `j/k` or the arrow keys, and switch focus between the list and the detail
pane with `Tab`. The detail pane also scrolls with `PgUp/PgDn`, and `q` or `Esc` exits.
No operation on this screen modifies an Issue or its related data.

### Projects and Milestones

Group a finite outcome as a Project, and for a Project that needs stages, create ordered
Milestones. Projects and Milestones can be referenced by name or by number.
`project list` returns every Project including closed ones by default, and each tally counts
all Issues, closed ones included, as open / closed. When you only want the Projects being
worked on, say so with `project list --active`. Whether a Project is closed is set with
`project create --closed` and `project set-state <project> <state> --closed`.
Projects are displayed in creation order. If you need priority, define a `single` label group
of your own.

```sh
octa project create --name "Publish the CLI"
octa project list
octa project list --active

octa milestone create --project "Publish the CLI" \
  --name "Public beta" \
  --description "The stage that ships a beta to users" \
  --status active \
  --position 1 \
  --target-date 2026-09-01

octa milestone list --project "Publish the CLI"
octa milestone show "Public beta" --project "Publish the CLI"
octa milestone set "Public beta" --project "Publish the CLI" \
  --status completed \
  --target-date 2026-09-15
```

A Project and a Milestone can be set when the Issue is created. A Milestone is an entity
inside a Project, so `--milestone` also requires `--project`.

```sh
octa issue open \
  --title "Invite beta users" \
  --project "Publish the CLI" \
  --milestone "Public beta"
```

You can also set or unset a Milestone on an existing Issue, and list the Issues belonging to
the same Milestone. To change or clear the Project, unset the Milestone first.

```sh
octa issue set 1 --milestone "Public beta" --lease "$LEASE"
octa issue list --project "Publish the CLI" --milestone "Public beta"
octa issue unset 1 --milestone --lease "$LEASE"
```

Parent-child relationships between Issues can be set within the same repository and are
independent of Project membership. Parent and child may belong to different Projects, and it
is fine for only one of them to belong to a Project at all. When a parent is set on an
existing Issue that has no Project, the parent's Project at that moment is inherited as the
initial value; afterwards the Project of the parent and of the child can each be changed or
cleared.

```sh
LEASE_2=$(octa issue lock 2)
octa issue set 2 --parent 1 --lease "$LEASE_2"
octa issue set 2 --project "Another Project" --lease "$LEASE_2"
octa issue unset 1 --project --lease "$LEASE"
```

Only for an Issue that has a Milestone inside a Project does the old rule still apply: unset
the Milestone first, then change or clear the Project.

A new repository is seeded with one default state per type, plus a second way to end.

| State | Type | Default for that type |
|---|---|---|
| open | open | ✓ |
| in progress | in progress | ✓ |
| closed | closed | ✓ |
| not planned | closed | |

Seeding only runs for a repository that has no states at all. The state configuration of a
repository that already has a workflow is left as it is.

States can be added, changed, and deleted later. `--type` defaults to `open` when omitted.

```sh
octa config state create Backlog
octa config state create "In Review" --type "in progress"
octa config state set open --name Ready
octa config state set Ready --type "in progress"
octa config state delete Backlog --move-to Ready
octa config state set Ready --default
octa config state list
```

Renaming with `config state set --name` moves the Issues in that state along with it.
`config state delete` requires `--move-to <state>` when Issues remain in the state.
Both are also reference constraints in the database: deleting a state that still holds Issues
is rejected. Deleting a state never takes Issues down with it.

**A type that has any state always has exactly one default state. The database schema
guarantees this.** Creating the first state in an empty type makes that state the default even
without `--default`. The same happens when a state is moved into an empty type. A verb invoked
without arguments must always have a determined destination, as long as the type has a usable
state. This automatic promotion is reported in the output.

To move the default to a different state, use `config state set <name> --default`.
`config state create --default` behaves the same way. Neither takes a type argument, because a
state already has exactly one type and restating it could only produce a contradiction.

While another state remains in the same type, you cannot delete the default state or change its
type. Move the default first with `config state set <name> --default`. This too is a schema-level
constraint. In addition, the `open` and `closed` types cannot be emptied, because every Issue must
be able to start and to finish. The `in progress` type may be empty, since a workflow that does not
distinguish started work is valid. In that case `issue start` has no destination and fails,
reporting that no state of the `in progress` type exists. Create one state and it becomes the
default, so `issue start` works again as-is.

States carry no ordering. The display order of `config state list` is derived from type, default
flag, and name, running `open` → `in progress` → `closed`. Within each type the default state comes
first and the rest follow in name order.

Whether an Issue is closed is derived from its type. An Issue in a state whose `type` is `closed`
is closed. That is what `issue list --state-type closed` and the `Open/Closed` columns of
`project list` count. The reason it was closed is expressed by the state name, not the type — which
is why `closed` and `not planned` share a type.

This type axis is the only classification octa holds over states; it does not distinguish stages in
between. State names themselves are given no meaning, so any state name can be used.
Workflow states and other custom states created by older versions, along with the Issues that
reference them, are neither deleted nor renamed by a migration.

Issue states are transitioned with verbs. A verb invoked without arguments moves the Issue to the
default of its type.

```sh
octa issue start 1 --lease "$LEASE"
octa issue close 1 --lease "$LEASE"
octa issue close 1 --as "not planned" --lease "$LEASE"
octa issue reopen 1 --lease "$LEASE"
octa issue set 1 --as "In Review" --lease "$LEASE"
```

`--as` only accepts states belonging to that verb's type. The same holds for `issue open` (aliased
as `create`), which accepts only open-type states. To record an Issue that has already been started
or already resolved, `open` it and then `start` or `close` it. `issue set --as` is the only
operation that can move an Issue to any state regardless of type. `issue start` has no `--as`
because the destination is not yet determined at the moment work begins.

### Dependencies

To record that Issue 1 blocks Issue 2:

```sh
octa issue add 1 --blocks 2 --lease "$LEASE"
octa issue show 1
octa issue show 2
```

Work that has no blocker outside a terminal state can be listed with `--unblocked`.

```sh
octa issue list --unblocked
```

To drop a dependency, `remove` the same property.

```sh
octa issue remove 1 --blocks 2 --lease "$LEASE"
```

Related Issues with no ordering between them are connected with `--related`. Adding the same pair in
the opposite order stores a single record, and `--related-to` narrows the candidates.

```sh
octa issue add 1 --related 2 --lease "$LEASE"
octa issue list --related-to 1
octa issue remove 2 --related 1 --lease "$LEASE_2"
```

### Labels

Labels can be used on their own.
Label names and group names are yours to choose per repository; octa reserves no classification
names and gives no special treatment to labels such as `impl` / `design` / `research`.

```sh
octa config label create documentation --target issue
octa issue add 1 --label documentation --lease "$LEASE"
octa issue remove 1 --label documentation --lease "$LEASE"
```

In a `single` group, only one label from that group can be attached at a time.

In a `multi` group, several labels from the same group can coexist.

```sh
octa config label-group create priority --target issue --selection single
octa config label create high --target issue --group priority
octa config label create low --target issue --group priority
octa issue add 1 --label high --lease "$LEASE"

octa config label-group create area --target issue --selection multi
octa config label create cli --target issue --group area
octa config label create storage --target issue --group area
octa issue add 1 --label cli --lease "$LEASE"
octa issue add 1 --label storage --lease "$LEASE"
```

Label definitions for Projects are separate from those for Issues. The same name can be defined for
each independently, and `--target` is required.

```sh
octa config label-group create horizon --target project --selection single
octa config label create now --target project --group horizon
octa config label create next --target project --group horizon
octa project add "Publish the CLI" --label now
octa project remove "Publish the CLI" --label now
```

## Keeping Pull Request discussion

A Pull Request in octa is a numbered discussion entity tied to a branch.

Git handles the code and the diff; octa holds the state and the comments.

```sh
octa pr create \
  --title "Add the release process" \
  --branch docs/release-process \
  --body "Update the Wiki and the README." \
  --issue 1 \
  --lease "$LEASE"

octa pr comment 1 --body "Please take a look."
octa pr show 1
octa pr set-state 1 closed
```

An existing PR keeps working exactly as it was created, and can be explicitly linked to an Issue only
when that becomes necessary.
One Issue can link to several PRs and one PR to several Issues. The same pair is never stored twice.

```sh
octa pr add 2 --issue 1 --lease "$LEASE"
octa issue show 1
octa pr remove 2 --issue 1 --lease "$LEASE"
```

PR listings can be filtered with `open`, `closed`, or `all`.

```sh
octa pr list --state open
octa pr list --state all
```

## Keeping policies and procedures in the Wiki

The Wiki is stored in octa's local store, not as files inside the repository.

When the slug is omitted, one is generated from the title using ASCII alphanumerics and hyphens.

For titles where that generation yields an empty slug — a title written only in Japanese, for
example — pass `--slug` explicitly.

```sh
octa wiki create \
  --title "Release process" \
  --slug release-process \
  --body "See [[development-policy]] for the related policy."

octa wiki show release-process
octa wiki list
```

You can also specify an explicit slug.

```sh
octa wiki create \
  --title "Development policy" \
  --slug development-policy \
  --body "Design decisions are recorded here."
```

A `[[slug]]` in the body is recorded as a link.

`wiki show` displays both the links from that page and the backlinks to it.

## Reading exactly the data you need with GraphQL

`octa query` exposes a read-only GraphQL schema scoped to the current repository by default.
The document is passed on standard input or with `--file`, and variables are given as a JSON object.

```sh
octa query --variables '{"number": 25}' <<'GRAPHQL'
query IssueContext($number: Int!) {
  issue(number: $number) {
    number
    title
    leased
    project { name }
    labels { name }
    blocks(limit: 20) { number title }
  }
}
GRAPHQL

octa query --file query.graphql --variables '{"limit": 20}'
```

The selection set is translated into a SQLite query that fetches only the columns and relations you
asked for. A single relation becomes a correlated JOIN and a multiple relation an aggregate subquery
containing a JOIN; relations that were not selected are never accessed. The response is a GraphQL
JSON envelope, with the number of executed queries in `extensions.dbAccesses`. The `limit` of a list
field defaults to 50 and caps at 100; query depth caps at 8 and complexity at 500. The schema has no
mutations. An Issue's `leased` field only reports whether a lease is held; it never exposes the
lease ID.

Both success and validation errors come back as a standard GraphQL JSON envelope.
Success carries `data` and a validation error carries `errors`, so check the envelope rather than the
CLI exit status alone.

```json
{"data":{"issue":{"number":25,"title":"Add a read-only GraphQL query surface"}},"extensions":{"dbAccesses":1}}
```

```sh
printf '%s\n' '{ missingField }' | octa query
```

```json
{"data":null,"extensions":{"dbAccesses":0},"errors":[{"message":"Unknown field \"missingField\" on type \"QueryRoot\".","locations":[{"line":1,"column":3}]}]}
```

The available types and fields can be inspected through introspection or SDL output.

```sh
octa query --schema
```

## JSON output

For automation and agents, pass `--json` to the commands that support it.

```sh
octa issue create --title "Investigate" --json
octa issue list --all --json
octa issue show 1 --json
octa pr list --state all --json
octa wiki show release-process --json
octa config label list --target issue --json
```

## Worktrees and repository scope

Normally the target is the Git repository you are currently in.

Because the Git common directory is used as the identifier, multiple worktrees of the same repository
share the same octa data.

To name another registered repository explicitly, use `--repo`.

```sh
octa --repo other-repository issue list
```

Some read-only listings can span registered repositories with `--all-repos`.

```sh
octa --all-repos issue list --all
octa --all-repos pr list --state all
octa --all-repos wiki list
```

Mutating operations, and Issue filtering by `--label`, `--project`, `--milestone`, `--related-to`, and
`--unblocked`, must be run against a single repository. State configuration is global across
repositories, so `--state` and `--state-type` also work with `--all-repos`.

## Storage location and backups

octa uses one SQLite database per user.

```text
$XDG_DATA_HOME/octa/octa.db
```

When `XDG_DATA_HOME` is unset, the location is:

```text
~/.local/share/octa/octa.db
```

This database is not committed to Git and is not synced automatically to clones or remotes.

If you need to migrate machines or keep backups, back this database up.

## Discovering commands

```sh
octa --help
octa issue --help
octa issue add --help
octa milestone --help
octa pr --help
octa wiki --help
octa config label --help
octa config label-group --help
octa config state --help
```

A guide for AI agents to discover and use the octa CLI's features, scope, JSON output, and storage
location lives in [`skills/octa`](skills/octa/SKILL.md). Team-specific Issue conventions are kept out
of that guide.

## Checks during development

```sh
cargo fmt --check
SQLX_OFFLINE=true cargo clippy --all-targets -- -D warnings
SQLX_OFFLINE=true TMPDIR=/private/tmp cargo test
```
