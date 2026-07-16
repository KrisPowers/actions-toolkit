CREATE TABLE artifacts (
    id              TEXT PRIMARY KEY,
    workflow_run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    job_run_id      TEXT REFERENCES job_runs(id),
    name            TEXT NOT NULL,
    path_on_disk    TEXT NOT NULL,
    size_bytes      INTEGER NOT NULL,
    content_type    TEXT,
    created_at      TEXT NOT NULL
);
