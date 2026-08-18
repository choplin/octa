-- octa stores every repository in one SQLite database. Issues, PRs, projects,
-- and wiki pages are repository-scoped, so those tables carry repo_id
-- explicitly. Configuration -- issue states, labels, and label groups -- is
-- global: one set governs every repository.

CREATE TABLE repos (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_key TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

-- A state is classified on one axis: its type. The three values are the
-- distinctions a user already holds before meeting octa -- not yet resolved,
-- picked up, resolved -- so octa deliberately models no finer gradation.
-- Whether a state closes an issue is read from `type = 'closed'`; it is not a
-- second stored flag.
CREATE TABLE issue_states (
    name TEXT NOT NULL PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('open', 'in progress', 'closed'))
);

-- Which state each type hands out when a verb is invoked without an explicit
-- target: `issue open` resolves the open default, `start` the in-progress one,
-- `close` the closed one.
--
-- The default is one fact per type, so it is one row per type rather than a
-- flag spread across `issue_states`. That is what makes it constrainable:
-- `PRIMARY KEY (type)` admits at most one, the reference keeps it pointing at a
-- state that exists, and changing it is a single-row write with no moment in
-- between where the type has none.
CREATE TABLE issue_state_defaults (
    type TEXT NOT NULL PRIMARY KEY CHECK (type IN ('open', 'in progress', 'closed')),
    name TEXT NOT NULL UNIQUE
        REFERENCES issue_states(name) ON UPDATE CASCADE ON DELETE CASCADE
);

-- A CHECK constraint cannot see other rows, so the rules that relate the two
-- tables are triggers. Together they hold one invariant: a type with any states
-- has exactly one default, and that default is a state of that same type.

-- The reference alone allows a default of one type to name a state of another.
CREATE TRIGGER issue_state_defaults_belong_to_their_type_insert
AFTER INSERT ON issue_state_defaults
WHEN NOT EXISTS (
    SELECT 1 FROM issue_states s WHERE s.name = NEW.name AND s.type = NEW.type)
BEGIN
    SELECT RAISE(ABORT, 'a default state must belong to the type it is default for');
END;

CREATE TRIGGER issue_state_defaults_belong_to_their_type_update
AFTER UPDATE ON issue_state_defaults
WHEN NOT EXISTS (
    SELECT 1 FROM issue_states s WHERE s.name = NEW.name AND s.type = NEW.type)
BEGIN
    SELECT RAISE(ABORT, 'a default state must belong to the type it is default for');
END;

-- A type gains its default the moment it gains a state, so a populated type is
-- never left with a verb that has nowhere to go.
CREATE TRIGGER issue_states_first_of_a_type_becomes_its_default
AFTER INSERT ON issue_states
WHEN NOT EXISTS (SELECT 1 FROM issue_state_defaults d WHERE d.type = NEW.type)
BEGIN
    INSERT INTO issue_state_defaults (type, name) VALUES (NEW.type, NEW.name);
END;

-- A state that changes type stops being the old type's default and, when the
-- new type had none, becomes its default. The delete is what asks whether the
-- old type was allowed to lose it: by now the state has already left, so a type
-- emptied by the move is free to go without one.
CREATE TRIGGER issue_states_retype_hands_over_the_default
AFTER UPDATE OF type ON issue_states
BEGIN
    DELETE FROM issue_state_defaults WHERE name = NEW.name AND type = OLD.type;
    INSERT INTO issue_state_defaults (type, name)
    SELECT NEW.type, NEW.name
    WHERE NOT EXISTS (SELECT 1 FROM issue_state_defaults d WHERE d.type = NEW.type);
END;

-- Deleting a state cascades its default row away. Losing it is only legal when
-- the state was the last of its type; otherwise the type would be left with
-- states and no default.
CREATE TRIGGER issue_state_defaults_a_populated_type_keeps_one
AFTER DELETE ON issue_state_defaults
WHEN EXISTS (SELECT 1 FROM issue_states s WHERE s.type = OLD.type)
BEGIN
    SELECT RAISE(ABORT, 'a state type that still has states must keep a default state');
END;

-- `state` references a configured state by name rather than by id, so the name
-- is the only handle the rest of the system needs. The two referential actions
-- carry that choice: renaming a state moves its issues with it, and a state
-- that still has issues cannot be deleted out from under them. Together they
-- make an issue in an unconfigured state unrepresentable, which is why every
-- read joins `issue_states` directly instead of guarding against a missing row.
CREATE TABLE issues (
    repo_id    INTEGER NOT NULL REFERENCES repos(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    state      TEXT NOT NULL
        REFERENCES issue_states(name) ON UPDATE CASCADE ON DELETE RESTRICT,
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
    name      TEXT NOT NULL PRIMARY KEY,
    selection TEXT NOT NULL CHECK (selection IN ('single', 'multi'))
);

CREATE TABLE labels (
    name       TEXT NOT NULL PRIMARY KEY,
    group_name TEXT
);

CREATE TABLE issue_labels (
    repo_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    label_name   TEXT NOT NULL,
    PRIMARY KEY (repo_id, issue_number, label_name),
    FOREIGN KEY (repo_id, issue_number)
        REFERENCES issues(repo_id, number),
    FOREIGN KEY (label_name)
        REFERENCES labels(name)
);

CREATE TABLE project_label_groups (
    name      TEXT NOT NULL PRIMARY KEY,
    selection TEXT NOT NULL CHECK (selection IN ('single', 'multi'))
);

CREATE TABLE project_labels (
    name       TEXT NOT NULL PRIMARY KEY,
    group_name TEXT
        REFERENCES project_label_groups(name)
);

-- Project states are the same idea as `issue_states`, narrowed to two types.
-- The type axis exists to give a transition verb somewhere to go and to answer
-- whether the project is closed, and a project has no third thing to be: it is
-- an outcome that is either still open or finished with. Whether a project has
-- work under way is already readable from its issue tally, so an `in progress`
-- type would restate a derived signal rather than constrain anything. Naming a
-- state `Planned` or `In Progress` remains a matter of the name.
CREATE TABLE project_states (
    name TEXT NOT NULL PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('open', 'closed'))
);

-- Which state each type hands out when a verb is invoked without an explicit
-- target: `project create` resolves the open default, `close` the closed one.
-- One fact per type, so one row per type, for the reasons `issue_state_defaults`
-- records.
CREATE TABLE project_state_defaults (
    type TEXT NOT NULL PRIMARY KEY CHECK (type IN ('open', 'closed')),
    name TEXT NOT NULL UNIQUE
        REFERENCES project_states(name) ON UPDATE CASCADE ON DELETE CASCADE
);

-- The same four rules that hold `issue_state_defaults` together, over two types
-- instead of three: a type with any states has exactly one default, and that
-- default is a state of that same type.
CREATE TRIGGER project_state_defaults_belong_to_their_type_insert
AFTER INSERT ON project_state_defaults
WHEN NOT EXISTS (
    SELECT 1 FROM project_states s WHERE s.name = NEW.name AND s.type = NEW.type)
BEGIN
    SELECT RAISE(ABORT, 'a default state must belong to the type it is default for');
END;

CREATE TRIGGER project_state_defaults_belong_to_their_type_update
AFTER UPDATE ON project_state_defaults
WHEN NOT EXISTS (
    SELECT 1 FROM project_states s WHERE s.name = NEW.name AND s.type = NEW.type)
BEGIN
    SELECT RAISE(ABORT, 'a default state must belong to the type it is default for');
END;

CREATE TRIGGER project_states_first_of_a_type_becomes_its_default
AFTER INSERT ON project_states
WHEN NOT EXISTS (SELECT 1 FROM project_state_defaults d WHERE d.type = NEW.type)
BEGIN
    INSERT INTO project_state_defaults (type, name) VALUES (NEW.type, NEW.name);
END;

CREATE TRIGGER project_states_retype_hands_over_the_default
AFTER UPDATE OF type ON project_states
BEGIN
    DELETE FROM project_state_defaults WHERE name = NEW.name AND type = OLD.type;
    INSERT INTO project_state_defaults (type, name)
    SELECT NEW.type, NEW.name
    WHERE NOT EXISTS (SELECT 1 FROM project_state_defaults d WHERE d.type = NEW.type);
END;

CREATE TRIGGER project_state_defaults_a_populated_type_keeps_one
AFTER DELETE ON project_state_defaults
WHEN EXISTS (SELECT 1 FROM project_states s WHERE s.type = OLD.type)
BEGIN
    SELECT RAISE(ABORT, 'a state type that still has states must keep a default state');
END;

-- `state` references a configured state by name for the reasons `issues.state`
-- records, and whether the project is closed is read from that state's type
-- rather than stored beside it. A separate flag could disagree with the name;
-- this cannot.
CREATE TABLE projects (
    repo_id      INTEGER NOT NULL,
    id           INTEGER NOT NULL,
    name         TEXT NOT NULL COLLATE NOCASE,
    summary      TEXT NOT NULL DEFAULT '',
    description  TEXT NOT NULL DEFAULT '',
    state        TEXT NOT NULL
        REFERENCES project_states(name) ON UPDATE CASCADE ON DELETE RESTRICT,
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
    FOREIGN KEY (label_name)
        REFERENCES project_labels(name)
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
