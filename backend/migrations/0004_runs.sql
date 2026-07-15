CREATE TABLE workflow_runs (
    id                      TEXT PRIMARY KEY,
    workflow_id             TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    repo_id                 TEXT NOT NULL REFERENCES repos(id),
    trigger_event           TEXT NOT NULL,
    trigger_payload_json    TEXT,
    ref_name                TEXT,
    commit_sha               TEXT,
    status                    TEXT NOT NULL,
    started_at                 TEXT,
    finished_at                  TEXT,
    created_at                    TEXT NOT NULL
);

CREATE TABLE job_runs (
    id                  TEXT PRIMARY KEY,
    workflow_run_id     TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
    job_key             TEXT NOT NULL,
    name                TEXT,
    status              TEXT NOT NULL,
    needs_json          TEXT NOT NULL DEFAULT '[]',
    container_id        TEXT,
    started_at          TEXT,
    finished_at         TEXT,
    exit_code           INTEGER
);

CREATE TABLE step_runs (
    id              TEXT PRIMARY KEY,
    job_run_id      TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    step_index      INTEGER NOT NULL,
    name            TEXT,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT,
    exit_code       INTEGER
);
