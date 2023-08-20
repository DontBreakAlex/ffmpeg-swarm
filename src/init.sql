CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    args TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    finished_at TIMESTAMP
);

CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL,
    inputs TEXT NOT NULL,
    output TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    finished_at TIMESTAMP,
    exit_code INTEGER,
    FOREIGN KEY (task_id) REFERENCES tasks
);

CREATE TABLE IF NOT EXISTS known_peers (
    uuid BLOB PRIMARY KEY ON CONFLICT REPLACE,
    ips BLOB NOT NULL,
    oldest_job TIMESTAMP
);

UPDATE jobs SET started_at = NULL WHERE finished_at IS NULL;