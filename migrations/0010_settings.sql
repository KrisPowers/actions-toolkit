-- Runtime settings (port, bind address, Docker host override, max concurrent jobs) moved out of
-- .env/CLI-only config into a DB-backed singleton row, seeded with defaults by this migration so
-- the settings exist as soon as the database is created, before the server ever binds.

CREATE TABLE settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    port                INTEGER NOT NULL DEFAULT 7890,
    bind_addr           TEXT NOT NULL DEFAULT '0.0.0.0',
    docker_host         TEXT,
    max_concurrent_jobs INTEGER NOT NULL DEFAULT 4,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

INSERT INTO settings (id, port, bind_addr, docker_host, max_concurrent_jobs, created_at, updated_at)
VALUES (1, 7890, '0.0.0.0', NULL, 4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
