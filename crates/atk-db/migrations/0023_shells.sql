-- One shell per workflow run triggered inside a bucket: a real OS subprocess (local, or on a
-- remote agent once clustering lands) that drives that run's job DAG and talks back to the
-- control plane over RCP instead of touching the database directly.

CREATE TABLE shells (
    id                  TEXT PRIMARY KEY,
    bucket_id           TEXT NOT NULL REFERENCES buckets(id) ON DELETE CASCADE,
    workflow_run_id     TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    agent_id            TEXT,
    target_os           TEXT NOT NULL,
    pid                 INTEGER,
    status              TEXT NOT NULL,
    exit_code           INTEGER,
    started_at          TEXT NOT NULL,
    finished_at         TEXT,
    outcome_persisted_at TEXT,
    reaped_at           TEXT
);

CREATE INDEX idx_shells_bucket ON shells (bucket_id);
CREATE INDEX idx_shells_reaped ON shells (reaped_at, finished_at);
CREATE INDEX idx_shells_workflow_run ON shells (workflow_run_id);
