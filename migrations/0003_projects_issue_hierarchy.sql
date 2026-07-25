-- Repo-scoped finite outcomes and one-to-many issue hierarchy. Associations
-- are join tables so the existing issues table remains backward compatible.
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
    FOREIGN KEY (repo_id) REFERENCES repos(id)
);

CREATE TABLE issue_projects (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    project_id   INTEGER NOT NULL,
    PRIMARY KEY (repo_id, issue_number),
    FOREIGN KEY (repo_id, issue_number) REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, project_id) REFERENCES projects(repo_id, id) ON DELETE RESTRICT
);

CREATE TABLE issue_parents (
    repo_id       INTEGER NOT NULL,
    child_number  INTEGER NOT NULL,
    parent_number INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, child_number),
    CHECK (child_number <> parent_number),
    FOREIGN KEY (repo_id, child_number) REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, parent_number) REFERENCES issues(repo_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_projects_project_idx
    ON issue_projects (repo_id, project_id, issue_number);
CREATE INDEX issue_parents_parent_idx
    ON issue_parents (repo_id, parent_number, child_number);

-- Reject an edge when its proposed parent is already below the child.
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
        SELECT 1 FROM ancestors WHERE number = NEW.child_number
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
        SELECT 1 FROM ancestors WHERE number = NEW.child_number
    );
END;
