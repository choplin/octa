-- octa stores every repository in one SQLite database. Entity identifiers are
-- repository-scoped, so each table carries repo_id explicitly.

CREATE TABLE repos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_key TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE issue_states (
    repo_id     INTEGER NOT NULL REFERENCES repos(id),
    name        TEXT NOT NULL,
    status_type TEXT NOT NULL DEFAULT 'unstarted'
        CHECK (status_type IN ('backlog', 'unstarted', 'started', 'completed', 'canceled')),
    is_starting INTEGER NOT NULL DEFAULT 0,
    is_terminal INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (repo_id, name)
);

CREATE TABLE issues (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    state      TEXT NOT NULL,
    priority   INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, number)
);

CREATE TABLE issue_leases (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    lease_id     TEXT NOT NULL UNIQUE,
    acquired_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, issue_number),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE
);

CREATE TABLE comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number)
);

CREATE TABLE issue_deps (
    repo_id        INTEGER NOT NULL,
    blocker_number INTEGER NOT NULL,
    blocked_number INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, blocker_number, blocked_number),
    FOREIGN KEY (repo_id, blocker_number)
        REFERENCES issues(repo_id, number),
    FOREIGN KEY (repo_id, blocked_number)
        REFERENCES issues(repo_id, number)
);

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
    FOREIGN KEY (repo_id, pr_number)
        REFERENCES prs(repo_id, number)
);

CREATE TABLE wiki_pages (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    slug       TEXT NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, slug)
);

CREATE TABLE wiki_links (
    repo_id   INTEGER NOT NULL,
    from_slug TEXT NOT NULL,
    to_slug   TEXT NOT NULL,
    PRIMARY KEY (repo_id, from_slug, to_slug),
    FOREIGN KEY (repo_id, from_slug)
        REFERENCES wiki_pages(repo_id, slug)
);

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
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number),
    FOREIGN KEY (repo_id, label_name)
        REFERENCES labels(repo_id, name)
);

CREATE TABLE project_label_groups (
    repo_id   INTEGER NOT NULL REFERENCES repos(id),
    name      TEXT NOT NULL,
    selection TEXT NOT NULL CHECK (selection IN ('single', 'multi')),
    PRIMARY KEY (repo_id, name)
);

CREATE TABLE project_labels (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    name       TEXT NOT NULL,
    group_name TEXT,
    PRIMARY KEY (repo_id, name),
    FOREIGN KEY (repo_id, group_name)
        REFERENCES project_label_groups(repo_id, name)
);

CREATE TABLE projects (
    repo_id      INTEGER NOT NULL,
    id           INTEGER NOT NULL,
    name         TEXT NOT NULL COLLATE NOCASE,
    summary      TEXT NOT NULL DEFAULT '',
    description  TEXT NOT NULL DEFAULT '',
    state        TEXT NOT NULL DEFAULT 'planned',
    status_type  TEXT NOT NULL DEFAULT 'unstarted'
        CHECK (status_type IN ('backlog', 'unstarted', 'started', 'completed', 'canceled')),
    priority     INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, id),
    UNIQUE (repo_id, name),
    FOREIGN KEY (repo_id)
        REFERENCES repos(id)
);

CREATE TABLE project_label_links (
    repo_id    INTEGER NOT NULL,
    project_id INTEGER NOT NULL,
    label_name TEXT NOT NULL,
    PRIMARY KEY (repo_id, project_id, label_name),
    FOREIGN KEY (repo_id, project_id)
        REFERENCES projects(repo_id, id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, label_name)
        REFERENCES project_labels(repo_id, name)
);

CREATE TABLE issue_projects (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    project_id   INTEGER NOT NULL,
    PRIMARY KEY (repo_id, issue_number),
    UNIQUE (repo_id, issue_number, project_id),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, project_id)
        REFERENCES projects(repo_id, id) ON DELETE RESTRICT
);

CREATE INDEX issue_projects_project_idx
    ON issue_projects (repo_id, project_id, issue_number);

CREATE TABLE issue_parents (
    repo_id       INTEGER NOT NULL,
    child_number  INTEGER NOT NULL,
    parent_number INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, child_number),
    CHECK (child_number <> parent_number),
    FOREIGN KEY (repo_id, child_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, parent_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_parents_parent_idx
    ON issue_parents (repo_id, parent_number, child_number);

CREATE TRIGGER issue_parents_no_cycle_insert
BEFORE INSERT ON issue_parents
BEGIN
    SELECT RAISE(ABORT, 'issue parent cycle')
    WHERE EXISTS (
        WITH RECURSIVE ancestors(number) AS (
            SELECT NEW.parent_number
            UNION ALL
            SELECT p.parent_number
            FROM issue_parents p
            JOIN ancestors a
              ON p.repo_id = NEW.repo_id
             AND p.child_number = a.number
        )
        SELECT 1
        FROM ancestors
        WHERE number = NEW.child_number
    );
END;

CREATE TRIGGER issue_parents_no_cycle_update
BEFORE UPDATE OF parent_number, child_number, repo_id ON issue_parents
BEGIN
    SELECT RAISE(ABORT, 'issue parent cycle')
    WHERE EXISTS (
        WITH RECURSIVE ancestors(number) AS (
            SELECT NEW.parent_number
            UNION ALL
            SELECT p.parent_number
            FROM issue_parents p
            JOIN ancestors a
              ON p.repo_id = NEW.repo_id
             AND p.child_number = a.number
             AND p.child_number <> OLD.child_number
        )
        SELECT 1
        FROM ancestors
        WHERE number = NEW.child_number
    );
END;

CREATE TABLE issue_relations (
    repo_id     INTEGER NOT NULL,
    low_number  INTEGER NOT NULL,
    high_number INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, low_number, high_number),
    CHECK (low_number < high_number),
    FOREIGN KEY (repo_id, low_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, high_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_relations_high_idx
    ON issue_relations (repo_id, high_number, low_number);

CREATE TABLE issue_pr_links (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    pr_number    INTEGER NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, issue_number, pr_number),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, pr_number)
        REFERENCES prs(repo_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_pr_links_pr_idx
    ON issue_pr_links (repo_id, pr_number, issue_number);

CREATE TABLE project_milestones (
    repo_id      INTEGER NOT NULL,
    project_id   INTEGER NOT NULL,
    id           INTEGER NOT NULL,
    position     INTEGER NOT NULL CHECK (position >= 0),
    name         TEXT NOT NULL COLLATE NOCASE,
    description  TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL DEFAULT 'planned',
    start_date   TEXT,
    target_date  TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, project_id, id),
    UNIQUE (repo_id, project_id, name),
    FOREIGN KEY (repo_id, project_id)
        REFERENCES projects(repo_id, id) ON DELETE CASCADE
);

CREATE INDEX project_milestones_order_idx
    ON project_milestones (repo_id, project_id, position, id);

CREATE TABLE issue_milestones (
    repo_id       INTEGER NOT NULL,
    issue_number  INTEGER NOT NULL,
    project_id    INTEGER NOT NULL,
    milestone_id  INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, issue_number),
    FOREIGN KEY (repo_id, issue_number, project_id)
        REFERENCES issue_projects(repo_id, issue_number, project_id)
        ON DELETE RESTRICT ON UPDATE RESTRICT,
    FOREIGN KEY (repo_id, project_id, milestone_id)
        REFERENCES project_milestones(repo_id, project_id, id)
        ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX issue_milestones_milestone_idx
    ON issue_milestones (repo_id, project_id, milestone_id, issue_number);
