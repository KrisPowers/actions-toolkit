-- Single-use, expiring tokens an operator issues (from the Agents UI) so a new worker machine can
-- join the cluster once and receive a CA-signed client certificate. The token itself is never
-- stored in plaintext, only its hash, same convention as password/secret storage elsewhere.

CREATE TABLE agent_join_tokens (
    id          TEXT PRIMARY KEY,
    token_hash  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);

CREATE INDEX idx_agent_join_tokens_unused ON agent_join_tokens (used_at, expires_at);
