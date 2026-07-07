-- SQLite schema for collaboration server
-- Extracted from db.rs init_schema(); preserved exactly (including column constraints)

CREATE TABLE IF NOT EXISTS teams (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    name TEXT NOT NULL,
    public_ip TEXT,
    public_port INTEGER,
    last_seen TEXT NOT NULL,
    is_online INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (team_id) REFERENCES teams(id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    url TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    claimed_by TEXT,
    claimed_at TEXT,
    progress REAL NOT NULL DEFAULT 0,
    local_path TEXT,
    file_size INTEGER,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (team_id) REFERENCES teams(id)
);

CREATE TABLE IF NOT EXISTS offline_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_devices_team ON devices(team_id);
CREATE INDEX IF NOT EXISTS idx_tasks_team ON tasks(team_id);
CREATE INDEX IF NOT EXISTS idx_offline_messages_device ON offline_messages(device_id);