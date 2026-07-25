-- Symmetric, repo-local issue relations are stored once as a canonical pair.
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

-- A workflow issue owns at most one PR, and a PR belongs to at most one issue.
-- Existing PRs remain unlinked until an explicit `pr link` or `pr create
-- --issue`; branch names are deliberately not used for migration backfill.
CREATE TABLE issue_pr_links (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    pr_number    INTEGER NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, issue_number),
    UNIQUE (repo_id, pr_number),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, pr_number)
        REFERENCES prs(repo_id, number) ON DELETE CASCADE
);
