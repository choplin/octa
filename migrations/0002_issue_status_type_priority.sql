-- Add Linear-compatible status categories and issue priority without changing
-- the existing state names or lifecycle defaults.
ALTER TABLE issue_states
ADD COLUMN status_type TEXT NOT NULL DEFAULT 'unstarted'
    CHECK (status_type IN ('backlog', 'unstarted', 'started', 'completed', 'canceled'));

-- Existing custom terminal states remain terminal in both representations.
UPDATE issue_states SET status_type = 'completed' WHERE is_terminal = 1;
UPDATE issue_states SET status_type = 'unstarted' WHERE name = 'open';
UPDATE issue_states SET status_type = 'started' WHERE name = 'in_progress';
UPDATE issue_states SET status_type = 'completed' WHERE name = 'closed';

ALTER TABLE issue_states
ADD COLUMN workflow_group TEXT NOT NULL DEFAULT 'active';

ALTER TABLE issue_states
ADD COLUMN workflow_rank INTEGER NOT NULL DEFAULT 100;

ALTER TABLE issues
ADD COLUMN priority INTEGER NOT NULL DEFAULT 0
    CHECK (priority BETWEEN 0 AND 4);

-- State names remain repository-owned data. The migration adds generic
-- categories without inserting or deleting any local workflow.
