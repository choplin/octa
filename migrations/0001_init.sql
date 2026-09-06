-- octa stores every repository in one SQLite database. Issues, pull requests,
-- and wiki pages are repository-scoped, so those tables carry repository_id
-- explicitly. Configuration -- issue states, labels, and label groups -- is
-- global: one set governs every repository.

-- `name` is the unique user-facing locator shared with the repository's local
-- Git config. `path` is the current canonical location and may change after a
-- move.
CREATE TABLE repositories (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- A state is classified on one axis: its type. The three values are the
-- distinctions a user already holds before meeting octa -- not yet resolved,
-- picked up, resolved -- so octa deliberately models no finer gradation.
-- Whether a state closes an issue is read from `type = 'closed'`; it is not a
-- second stored flag.
--
-- `UNIQUE (name, type)` adds no restriction over the primary key alone. It is
-- there to be referenced: it makes the pair a key that `issue_state_defaults`
-- can point at, which is what lets the type agreement between the two tables be
-- a foreign key rather than a trigger.
CREATE TABLE issue_states (
    name TEXT NOT NULL PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('open', 'in progress', 'closed')),
    UNIQUE (name, type)
);

-- Which state each type hands out when a verb is invoked without an explicit
-- target: `issue open` resolves the open default, `start` the in-progress one,
-- `close` the closed one.
--
-- The default is one fact per type, so it is one row per type rather than a
-- flag spread across `issue_states`. That is what makes it constrainable:
-- `PRIMARY KEY (type)` admits at most one, and changing it is a single-row
-- write with no moment in between where the type has none.
--
-- The reference carries both columns. A single-column one would keep the
-- default pointing at a state that exists while saying nothing about which type
-- that state has, so `('open', <a closed state>)` would satisfy it; naming the
-- pair rejects the mismatch outright. `ON UPDATE CASCADE` is what carries a
-- renamed state's default along with it.
CREATE TABLE issue_state_defaults (
    type TEXT NOT NULL PRIMARY KEY CHECK (type IN ('open', 'in progress', 'closed')),
    name TEXT NOT NULL UNIQUE,
    FOREIGN KEY (name, type) REFERENCES issue_states(name, type)
        ON UPDATE CASCADE ON DELETE CASCADE
);

-- A state's type is fixed at creation. Retyping one would have to hand the old
-- type's default over and answer whether that type was allowed to lose it, and
-- there is no use for it that deleting the state and creating the intended one
-- does not already serve -- that route states where the issues go, which
-- retyping decides silently.
CREATE TRIGGER issue_states_keep_their_type
BEFORE UPDATE OF type ON issue_states
WHEN NEW.type IS NOT OLD.type
BEGIN
    SELECT RAISE(ABORT, 'a state cannot change type; delete it and create the intended one');
END;

-- Deleting a state cascades its default row away. Losing it is only legal when
-- the state was the last of its type; otherwise the type would be left with
-- states and no default. A CHECK constraint cannot see the other table, so this
-- one rule remains a trigger.
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
    repository_id    INTEGER NOT NULL REFERENCES repositories(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    state      TEXT NOT NULL
        REFERENCES issue_states(name) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, number)
);

CREATE TABLE issue_leases (
    repository_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    lease_id     TEXT NOT NULL UNIQUE,
    acquired_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, issue_number),
    FOREIGN KEY (repository_id, issue_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE
);

CREATE TABLE issue_comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    repository_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repository_id, issue_number)
        REFERENCES issues(repository_id, number)
);

CREATE TABLE issue_dependencies (
    repository_id        INTEGER NOT NULL,
    blocker_number INTEGER NOT NULL,
    blocked_number INTEGER NOT NULL,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, blocker_number, blocked_number),
    FOREIGN KEY (repository_id, blocker_number)
        REFERENCES issues(repository_id, number),
    FOREIGN KEY (repository_id, blocked_number)
        REFERENCES issues(repository_id, number)
);

CREATE TABLE pull_requests (
    repository_id    INTEGER NOT NULL REFERENCES repositories(id),
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    branch     TEXT NOT NULL,
    state      TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, number)
);

CREATE TABLE pull_request_comments (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    repository_id    INTEGER NOT NULL,
    pull_request_number  INTEGER NOT NULL,
    body       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repository_id, pull_request_number)
        REFERENCES pull_requests(repository_id, number)
);

CREATE TABLE wiki_pages (
    repository_id    INTEGER NOT NULL REFERENCES repositories(id),
    slug       TEXT NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, slug)
);

CREATE TABLE wiki_links (
    repository_id   INTEGER NOT NULL,
    from_slug TEXT NOT NULL,
    to_slug   TEXT NOT NULL,
    PRIMARY KEY (repository_id, from_slug, to_slug),
    FOREIGN KEY (repository_id, from_slug)
        REFERENCES wiki_pages(repository_id, slug)
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
    repository_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    label_name   TEXT NOT NULL,
    PRIMARY KEY (repository_id, issue_number, label_name),
    FOREIGN KEY (repository_id, issue_number)
        REFERENCES issues(repository_id, number),
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
    type TEXT NOT NULL CHECK (type IN ('open', 'closed')),
    UNIQUE (name, type)
);

-- Which state each type hands out when a verb is invoked without an explicit
-- target: `project create` resolves the open default, `close` the closed one.
-- One fact per type, so one row per type, and the reference names the pair, for
-- the reasons `issue_state_defaults` records.
CREATE TABLE project_state_defaults (
    type TEXT NOT NULL PRIMARY KEY CHECK (type IN ('open', 'closed')),
    name TEXT NOT NULL UNIQUE,
    FOREIGN KEY (name, type) REFERENCES project_states(name, type)
        ON UPDATE CASCADE ON DELETE CASCADE
);

-- The same two rules that hold `issue_state_defaults` together, over two types
-- instead of three: a state keeps the type it was created with, and a type with
-- any states keeps exactly one default.
CREATE TRIGGER project_states_keep_their_type
BEFORE UPDATE OF type ON project_states
WHEN NEW.type IS NOT OLD.type
BEGIN
    SELECT RAISE(ABORT, 'a state cannot change type; delete it and create the intended one');
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
    repository_id      INTEGER NOT NULL,
    id           INTEGER NOT NULL,
    name         TEXT NOT NULL COLLATE NOCASE,
    summary      TEXT NOT NULL DEFAULT '',
    description  TEXT NOT NULL DEFAULT '',
    state        TEXT NOT NULL
        REFERENCES project_states(name) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, id),
    UNIQUE (repository_id, name),
    FOREIGN KEY (repository_id)
        REFERENCES repositories(id)
);

CREATE TABLE project_label_links (
    repository_id    INTEGER NOT NULL,
    project_id INTEGER NOT NULL,
    label_name TEXT NOT NULL,
    PRIMARY KEY (repository_id, project_id, label_name),
    FOREIGN KEY (repository_id, project_id)
        REFERENCES projects(repository_id, id) ON DELETE CASCADE,
    FOREIGN KEY (label_name)
        REFERENCES project_labels(name)
);

CREATE TABLE issue_projects (
    repository_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    project_id   INTEGER NOT NULL,
    PRIMARY KEY (repository_id, issue_number),
    UNIQUE (repository_id, issue_number, project_id),
    FOREIGN KEY (repository_id, issue_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repository_id, project_id)
        REFERENCES projects(repository_id, id) ON DELETE RESTRICT
);

CREATE INDEX issue_projects_project_index
    ON issue_projects (repository_id, project_id, issue_number);

CREATE TABLE issue_parents (
    repository_id       INTEGER NOT NULL,
    child_number  INTEGER NOT NULL,
    parent_number INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, child_number),
    CHECK (child_number <> parent_number),
    FOREIGN KEY (repository_id, child_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repository_id, parent_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_parents_parent_index
    ON issue_parents (repository_id, parent_number, child_number);

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
              ON p.repository_id = NEW.repository_id
             AND p.child_number = a.number
        )
        SELECT 1
        FROM ancestors
        WHERE number = NEW.child_number
    );
END;

CREATE TRIGGER issue_parents_no_cycle_update
BEFORE UPDATE OF parent_number, child_number, repository_id ON issue_parents
BEGIN
    SELECT RAISE(ABORT, 'issue parent cycle')
    WHERE EXISTS (
        WITH RECURSIVE ancestors(number) AS (
            SELECT NEW.parent_number
            UNION ALL
            SELECT p.parent_number
            FROM issue_parents p
            JOIN ancestors a
              ON p.repository_id = NEW.repository_id
             AND p.child_number = a.number
             AND p.child_number <> OLD.child_number
        )
        SELECT 1
        FROM ancestors
        WHERE number = NEW.child_number
    );
END;

CREATE TABLE issue_relations (
    repository_id     INTEGER NOT NULL,
    low_number  INTEGER NOT NULL,
    high_number INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, low_number, high_number),
    CHECK (low_number < high_number),
    FOREIGN KEY (repository_id, low_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repository_id, high_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_relations_high_index
    ON issue_relations (repository_id, high_number, low_number);

CREATE TABLE issue_pull_request_links (
    repository_id      INTEGER NOT NULL,
    issue_number INTEGER NOT NULL,
    pull_request_number    INTEGER NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, issue_number, pull_request_number),
    FOREIGN KEY (repository_id, issue_number)
        REFERENCES issues(repository_id, number) ON DELETE CASCADE,
    FOREIGN KEY (repository_id, pull_request_number)
        REFERENCES pull_requests(repository_id, number) ON DELETE CASCADE
);

CREATE INDEX issue_pull_request_links_pull_request_index
    ON issue_pull_request_links (repository_id, pull_request_number, issue_number);

CREATE TABLE project_milestones (
    repository_id      INTEGER NOT NULL,
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
    PRIMARY KEY (repository_id, project_id, id),
    UNIQUE (repository_id, project_id, name),
    FOREIGN KEY (repository_id, project_id)
        REFERENCES projects(repository_id, id) ON DELETE CASCADE
);

CREATE INDEX project_milestones_order_index
    ON project_milestones (repository_id, project_id, position, id);

CREATE TABLE issue_milestones (
    repository_id       INTEGER NOT NULL,
    issue_number  INTEGER NOT NULL,
    project_id    INTEGER NOT NULL,
    milestone_id  INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repository_id, issue_number),
    FOREIGN KEY (repository_id, issue_number, project_id)
        REFERENCES issue_projects(repository_id, issue_number, project_id)
        ON DELETE RESTRICT ON UPDATE RESTRICT,
    FOREIGN KEY (repository_id, project_id, milestone_id)
        REFERENCES project_milestones(repository_id, project_id, id)
        ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX issue_milestones_milestone_index
    ON issue_milestones (repository_id, project_id, milestone_id, issue_number);
