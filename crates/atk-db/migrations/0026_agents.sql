-- A worker machine registered for multi-machine shell execution. The control plane (UI + API +
-- DB) always stays on one machine; agents are where the heavier workflow-run workload can be
-- distributed to. `status='pending'` until an operator approves a freshly-joined agent;
-- `mtls_fingerprint` identifies the client certificate issued to it at join time.

CREATE TABLE agents (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    os                  TEXT NOT NULL,
    arch                TEXT NOT NULL,
    labels_json         TEXT NOT NULL DEFAULT '[]',
    capacity            INTEGER NOT NULL DEFAULT 1,
    mtls_fingerprint    TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending',
    last_heartbeat_at   TEXT,
    version             TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE(mtls_fingerprint)
);

CREATE INDEX idx_agents_status ON agents (status);
