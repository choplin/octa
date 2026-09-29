# Issue Leases

How does octa prevent two agents from concurrently changing the same Issue
without introducing users, sessions, or a remote lock service?

## The rule

An Issue may have at most one `issue_leases` row. Acquiring a lease inserts that
row and returns an opaque three-word `lease_id`; a primary-key conflict means
another holder already owns the Issue. The handle is shown once and is not an
actor identity, a renewable session, or an expiring timeout.

Operations that can replace or remove coordination state require the handle:

- state transitions and Issue title/body edits;
- Project, Milestone, and parent changes;
- label, dependency, and related-edge changes;
- comment deletion.

Opening an Issue and appending a comment do not require a lease. They create new
coordination evidence rather than overwrite an existing decision, so capture
and handoff remain possible even when an Issue is leased.

## Atomic validation and mutation

Protected operations call `begin_lease_mutation`. It begins a SQLite
transaction and executes a no-op `UPDATE` whose predicate includes repository,
Issue number, and lease ID. A matching row both proves ownership and obtains
SQLite's write lock. The operation then performs its writes and commits the
same transaction.

Validation must not be split into an earlier read followed by an unrelated
write transaction. A concurrent forced unlock could otherwise land between the
check and the mutation, allowing an invalidated holder to change the Issue.

Lease acquisition is also one conditional insert:

- the target Issue must exist;
- `ON CONFLICT DO NOTHING` makes contention return no handle;
- globally unique lease IDs are retried only when an ID collision, rather than
  Issue ownership, caused the conflict.

## Release and recovery

Ordinary unlock deletes only the row matching the supplied lease ID. A stale or
wrong handle changes nothing and reports failure. Forced unlock deletes by
Issue identity and exists solely as manual recovery when the handle is lost;
callers must establish that no active worker still relies on it.

Leases have no automatic expiry. Time-based expiry could transfer ownership
from a slow but live process without coordination, recreating the collision the
lease is intended to prevent. Explicit release and explicit forced recovery
keep that decision observable.

## Why not an assignee or label?

An assignee or `in-progress` label is descriptive data. Two contenders can both
read it as available and then both write ownership. The lease table makes the
invalid double-owner state unrepresentable at the storage boundary and gives
subsequent mutations a capability they can validate atomically.

## Verification

Tests must exercise competing acquisition, lease-ID collision retry, missing
and stale handles, forced unlock, and rollback of multi-statement protected
operations. End-to-end workflow tests must also prove that linked worktrees see
the same lease because they resolve the same repository and database.
