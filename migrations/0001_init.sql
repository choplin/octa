CREATE TABLE issues (
    number     INTEGER PRIMARY KEY AUTOINCREMENT,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    state      TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_number INTEGER NOT NULL REFERENCES issues(number),
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
