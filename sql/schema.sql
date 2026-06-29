-- Status values: 0 = Todo, 1 = InProgress, 2 = Done, 3 = Archived

CREATE TABLE IF NOT EXISTS tasks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    status          INTEGER NOT NULL DEFAULT 0 CHECK (status IN (0, 1, 2, 3)),
    priority        INTEGER NOT NULL DEFAULT 0 CHECK (priority >= 0),
    sort_order      REAL NOT NULL DEFAULT 0,
    due_at          TEXT,
    reminder_at     TEXT,
    parent_task_id  INTEGER REFERENCES tasks(id),
    project_id      INTEGER,
    assignee        INTEGER,
    completed_at    TEXT,
    created_by      INTEGER,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_by      INTEGER,
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_by      INTEGER,
    deleted_at      TEXT,
    archived_by     INTEGER,
    archived_at     TEXT
);

CREATE TABLE IF NOT EXISTS tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE IF NOT EXISTS task_tags (
    tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    PRIMARY KEY (tag_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_tasks_status_sort
    ON tasks(status, sort_order)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_task_tags_task_id ON task_tags(task_id);

CREATE INDEX IF NOT EXISTS idx_tasks_parent_task_id ON tasks(parent_task_id);
