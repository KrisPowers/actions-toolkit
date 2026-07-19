-- Tracks native sandbox ("Bucket") instances used to run job steps without Docker: one row per
-- job's sandbox, covering TTL-based auto-cleanup and startup crash reconciliation (a bucket row
-- still unreaped when the process starts back up came from a run that never got a chance to
-- clean up after itself, e.g. a crash).

CREATE TABLE buckets (
    id              TEXT PRIMARY KEY,
    job_run_id      TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    workflow_run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    os_pid          INTEGER,
    os_handle_json  TEXT,
    workspace_path  TEXT NOT NULL,
    network_enabled INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    ttl_expires_at  TEXT NOT NULL,
    reaped_at       TEXT
);

CREATE INDEX idx_buckets_reaped ON buckets (reaped_at, ttl_expires_at);
