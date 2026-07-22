-- workflow_runs.repo_id and webhook_events.repo_id referenced repos(id) without ON DELETE
-- CASCADE, so deleting a repo with any run or webhook-event history failed with a foreign key
-- constraint violation. workflows(repo_id) already cascades, but that only cleans up
-- workflow_runs indirectly through workflow_id; the direct repo_id reference on workflow_runs
-- still blocked the delete. SQLite has no ALTER TABLE for foreign key clauses, so both tables
-- are recreated with the corrected constraint.
--
-- Recreating workflow_runs means dropping it, and SQLite's DROP TABLE performs an implicit
-- delete of its rows first; with foreign_keys enabled that implicit delete cascades through
-- every table that references workflow_runs ON DELETE CASCADE (job_runs, artifacts, buckets)
-- and transitively through their own cascades (step_runs, run_logs), permanently wiping run
-- history this migration never intended to touch. Those tables are backed up into
-- unconstrained holding tables first and restored afterward so no data is lost. Recreating
-- webhook_events similarly requires clearing workflow_runs.webhook_event_id first, since that
-- reference has no cascade action and would otherwise block the drop outright; its original
-- values are backed up and restored the same way.

CREATE TABLE _migration_0020_job_runs AS SELECT * FROM job_runs;
CREATE TABLE _migration_0020_step_runs AS SELECT * FROM step_runs;
CREATE TABLE _migration_0020_run_logs AS SELECT * FROM run_logs;
CREATE TABLE _migration_0020_artifacts AS SELECT * FROM artifacts;
CREATE TABLE _migration_0020_buckets AS SELECT * FROM buckets;
CREATE TABLE _migration_0020_webhook_event_ids AS SELECT id, webhook_event_id FROM workflow_runs WHERE webhook_event_id IS NOT NULL;

CREATE TABLE workflow_runs_new (
    id                      TEXT PRIMARY KEY,
    workflow_id             TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    repo_id                 TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    trigger_event           TEXT NOT NULL,
    trigger_payload_json    TEXT,
    ref_name                TEXT,
    commit_sha              TEXT,
    status                  TEXT NOT NULL,
    started_at              TEXT,
    finished_at             TEXT,
    created_at              TEXT NOT NULL,
    webhook_event_id        TEXT REFERENCES webhook_events(id)
);

INSERT INTO workflow_runs_new SELECT * FROM workflow_runs;
DROP TABLE workflow_runs;
ALTER TABLE workflow_runs_new RENAME TO workflow_runs;

CREATE INDEX idx_workflow_runs_workflow ON workflow_runs(workflow_id, created_at DESC);
CREATE INDEX idx_workflow_runs_repo ON workflow_runs(repo_id, created_at DESC);

UPDATE workflow_runs SET webhook_event_id = NULL WHERE webhook_event_id IS NOT NULL;

CREATE TABLE webhook_events_new (
    id                      TEXT PRIMARY KEY,
    repo_id                 TEXT REFERENCES repos(id) ON DELETE CASCADE,
    github_event            TEXT NOT NULL,
    delivery_id             TEXT,
    payload_json            TEXT NOT NULL,
    signature_valid         INTEGER NOT NULL,
    matched_workflow_ids    TEXT NOT NULL DEFAULT '[]',
    received_at             TEXT NOT NULL,
    UNIQUE(delivery_id)
);

INSERT INTO webhook_events_new SELECT * FROM webhook_events;
DROP TABLE webhook_events;
ALTER TABLE webhook_events_new RENAME TO webhook_events;

CREATE INDEX idx_webhook_events_repo ON webhook_events(repo_id, received_at DESC);

UPDATE workflow_runs
SET webhook_event_id = (SELECT webhook_event_id FROM _migration_0020_webhook_event_ids b WHERE b.id = workflow_runs.id)
WHERE id IN (SELECT id FROM _migration_0020_webhook_event_ids);

INSERT INTO job_runs SELECT * FROM _migration_0020_job_runs;
INSERT INTO step_runs SELECT * FROM _migration_0020_step_runs;
INSERT INTO run_logs SELECT * FROM _migration_0020_run_logs;
INSERT INTO artifacts SELECT * FROM _migration_0020_artifacts;
INSERT INTO buckets SELECT * FROM _migration_0020_buckets;

DROP TABLE _migration_0020_job_runs;
DROP TABLE _migration_0020_step_runs;
DROP TABLE _migration_0020_run_logs;
DROP TABLE _migration_0020_artifacts;
DROP TABLE _migration_0020_buckets;
DROP TABLE _migration_0020_webhook_event_ids;
