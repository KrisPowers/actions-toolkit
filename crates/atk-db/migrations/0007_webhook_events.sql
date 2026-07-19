CREATE TABLE webhook_events (
    id                      TEXT PRIMARY KEY,
    repo_id                 TEXT REFERENCES repos(id),
    github_event            TEXT NOT NULL,
    delivery_id              TEXT,
    payload_json               TEXT NOT NULL,
    signature_valid              INTEGER NOT NULL,
    matched_workflow_ids           TEXT NOT NULL DEFAULT '[]',
    received_at                       TEXT NOT NULL,
    UNIQUE(delivery_id)
);
