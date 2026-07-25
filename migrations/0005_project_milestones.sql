-- Ordered Project phases and their optional issue assignment. Keeping the
-- Project id on the issue association lets SQLite enforce that an issue and
-- its milestone always belong to the same repository and Project.
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

-- SQLite requires the referenced columns to be covered by an exact UNIQUE
-- key. The issue_projects primary key is only (repo_id, issue_number), so add
-- the redundant composite key needed by issue_milestones.
CREATE UNIQUE INDEX issue_projects_issue_project_unique
    ON issue_projects (repo_id, issue_number, project_id);

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
