-- octa schema: a single global SQLite database holds every entity for every
-- repository. Each row carries a `repo_id` so the store is physically central
-- but logically scoped per repository.

-- Repositories: the logical per-repo scope within the global store.
-- `identity_key` is the canonicalized `git rev-parse --git-common-dir` path,
-- shared by every worktree of the same repository.
CREATE TABLE repos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_key TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Configurable issue states with starting (entry / "pickable") and terminal
-- ("done") flags. A default set is seeded per repo but is not locked.
CREATE TABLE issue_states (
    repo_id     INTEGER NOT NULL REFERENCES repos(id),
    name        TEXT NOT NULL,
    is_starting INTEGER NOT NULL DEFAULT 0,
    is_terminal INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (repo_id, name)
);

-- Issues: per-repo sequential number, description body, state, and an atomic
-- exclusive lock (`locked_by` NULL means unlocked).
CREATE TABLE issues (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    state      TEXT NOT NULL,
    locked_by  TEXT,
    locked_at  TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, number)
);

CREATE TABLE comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id, issue_number) REFERENCES issues(repo_id, number)
);

-- Dependency edges: `blocker_number` blocks `blocked_number` (blocked-by is the
-- same edge read from the other side).
CREATE TABLE issue_deps (
    repo_id        INTEGER NOT NULL,
    blocker_number INTEGER NOT NULL,
    blocked_number INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, blocker_number, blocked_number),
    FOREIGN KEY (repo_id, blocker_number) REFERENCES issues(repo_id, number),
    FOREIGN KEY (repo_id, blocked_number) REFERENCES issues(repo_id, number)
);

-- Pull requests: per-repo sequential number, tied to a git branch. octa stores
-- the discussion/review entity; the code and diff live on the git side.
CREATE TABLE prs (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    branch     TEXT NOT NULL,
    state      TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, number)
);

CREATE TABLE pr_comments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id    INTEGER NOT NULL,
    pr_number  INTEGER NOT NULL,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id, pr_number) REFERENCES prs(repo_id, number)
);

-- Wiki pages: octa entities (not repository files), addressed by a repo-scoped
-- slug and cross-linked via [[slug]] references in the body.
CREATE TABLE wiki_pages (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    slug       TEXT NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, slug)
);

-- Materialized [[slug]] links, re-derived from a page's body on every save.
CREATE TABLE wiki_links (
    repo_id   INTEGER NOT NULL,
    from_slug TEXT NOT NULL,
    to_slug   TEXT NOT NULL,
    PRIMARY KEY (repo_id, from_slug, to_slug),
    FOREIGN KEY (repo_id, from_slug) REFERENCES wiki_pages(repo_id, slug)
);

-- Label groups: `single` = mutually exclusive, `multi` = coexisting. Labels with
-- no group coexist freely.
CREATE TABLE label_groups (
    repo_id   INTEGER NOT NULL REFERENCES repos(id),
    name      TEXT NOT NULL,
    selection TEXT NOT NULL CHECK (selection IN ('single', 'multi')),
    PRIMARY KEY (repo_id, name)
);

CREATE TABLE labels (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    name       TEXT NOT NULL,
    group_name TEXT,
    PRIMARY KEY (repo_id, name)
);

CREATE TABLE issue_labels (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    label_name   TEXT NOT NULL,
    PRIMARY KEY (repo_id, issue_number, label_name),
    FOREIGN KEY (repo_id, issue_number) REFERENCES issues(repo_id, number),
    FOREIGN KEY (repo_id, label_name) REFERENCES labels(repo_id, name)
);
