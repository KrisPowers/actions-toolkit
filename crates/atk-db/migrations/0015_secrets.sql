-- Per-repo encrypted secrets a workflow step can reach as an env var, the same way GITHUB_TOKEN
-- is already injected. Scoped to a repo (not global) since different repos plausibly need
-- different credentials for the same-named service.

CREATE TABLE secrets (
    id              TEXT PRIMARY KEY,
    repo_id         TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    value_encrypted BLOB NOT NULL,
    value_nonce     BLOB NOT NULL,
    created_by      TEXT NOT NULL REFERENCES users(id),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE (repo_id, name)
);

CREATE INDEX idx_secrets_repo_id ON secrets(repo_id);
