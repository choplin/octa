-- Upgrade the original one-to-one link table additively so databases that
-- already applied 0004 keep every explicit link. No branch-name inference or
-- other backfill is performed.
CREATE TABLE issue_pr_links_many_to_many (
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

INSERT INTO issue_pr_links_many_to_many
    (repo_id, issue_number, pr_number, created_at)
SELECT repo_id, issue_number, pr_number, created_at
FROM issue_pr_links;

DROP TABLE issue_pr_links;

ALTER TABLE issue_pr_links_many_to_many RENAME TO issue_pr_links;

CREATE INDEX issue_pr_links_pr_idx
    ON issue_pr_links (repo_id, pr_number, issue_number);

-- Reconstructed pre-separation policy kept both cardinalities one-to-one.
CREATE UNIQUE INDEX issue_pr_links_one_issue_idx
    ON issue_pr_links (repo_id, issue_number);
CREATE UNIQUE INDEX issue_pr_links_one_pr_idx
    ON issue_pr_links (repo_id, pr_number);
