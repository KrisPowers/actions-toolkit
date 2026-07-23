-- Event-level bucket: the container for one triggering event (e.g. one push), which may fan out
-- to N matched workflow runs, each executing as its own shell subprocess inside this bucket.
-- `auth_token_hash` is a hash of the per-bucket bearer token handed to shells for RCP auth; the
-- token itself is generated with a CSPRNG and passed to each shell via its spec file, never
-- persisted in plaintext, same convention as the sandbox init spec.

CREATE TABLE buckets (
    id                TEXT PRIMARY KEY,
    trigger_kind      TEXT NOT NULL,
    webhook_event_id  TEXT REFERENCES webhook_events(id),
    repo_id           TEXT NOT NULL REFERENCES repos(id),
    status            TEXT NOT NULL,
    auth_token_hash   TEXT NOT NULL,
    rcp_endpoint      TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    completed_at      TEXT,
    reaped_at         TEXT
);

CREATE INDEX idx_buckets_reaped ON buckets (reaped_at);
CREATE INDEX idx_buckets_webhook_event ON buckets (webhook_event_id);
