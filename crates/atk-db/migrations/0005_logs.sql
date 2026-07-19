CREATE TABLE run_logs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    step_run_id     TEXT NOT NULL REFERENCES step_runs(id) ON DELETE CASCADE,
    ts              TEXT NOT NULL,
    stream          TEXT NOT NULL,
    message         TEXT NOT NULL
);

CREATE INDEX idx_run_logs_step ON run_logs(step_run_id, id);
