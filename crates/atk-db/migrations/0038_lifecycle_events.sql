-- Trip-wire timings for the runner's own event pipeline: bucket creation, RCP listener bind,
-- shell RCP handshake, checkout, shard/sandbox setup (down to the AppContainer profile, each ACL
-- grant, and the actual sandboxed process spawn/wait on Windows), and per-step execution. Each row
-- is written at the start of its phase (finished_at NULL) and updated once it ends, so a phase
-- that never gets its update -- a genuine hang, not just a slow-but-completed one -- shows up
-- directly as `finished_at IS NULL` long after `started_at`, instead of only being inferable after
-- the fact from a job's total duration. Recorded here rather than only logged, since the runner's
-- own log file is truncated on every restart -- see the investigation that motivated this table,
-- where a run's first step didn't start until ~9 minutes after the job did, with nothing anywhere
-- to say why.
--
-- subject_type/subject_id are polymorphic (bucket/shell/shard/job/step), matching audit_log's
-- target_type/target_id: no single FK fits all four, so neither is constrained. workflow_run_id is
-- nullable and denormalized onto every row (even shard/step-level ones) purely so a single query
-- can pull every phase for one run without joining back through job_runs/step_runs.
CREATE TABLE lifecycle_events (
    id              TEXT PRIMARY KEY,
    phase           TEXT NOT NULL,
    subject_type    TEXT NOT NULL,
    subject_id      TEXT NOT NULL,
    workflow_run_id TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
    detail          TEXT,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    ok              INTEGER,
    finish_detail   TEXT
);
CREATE INDEX idx_lifecycle_events_workflow_run_id ON lifecycle_events(workflow_run_id, started_at ASC);
CREATE INDEX idx_lifecycle_events_subject ON lifecycle_events(subject_type, subject_id);
-- The trip-wire query: phases that started but have no finish yet.
CREATE INDEX idx_lifecycle_events_unfinished ON lifecycle_events(finished_at) WHERE finished_at IS NULL;
