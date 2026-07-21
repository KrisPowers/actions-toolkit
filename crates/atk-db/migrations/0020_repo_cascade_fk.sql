-- workflow_runs.repo_id and webhook_events.repo_id referenced repos(id) without ON DELETE
-- CASCADE, so deleting a repo with any run or webhook-event history failed with a foreign key
-- constraint violation. workflows(repo_id) already cascades, but that only cleans up
-- workflow_runs indirectly through workflow_id; the direct repo_id reference on workflow_runs
-- still blocked the delete. SQLite has no ALTER TABLE for foreign key clauses, so both tables
-- are recreated with the corrected constraint and their data copied over.
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
