-- Replaces per-repo PATs with a single account-wide GitHub token entered during setup.
-- Repos keep their own webhook secret (webhooks are inherently per-repo), but no longer
-- carry credentials of their own.

CREATE TABLE github_token (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    token_encrypted     BLOB NOT NULL,
    token_nonce         BLOB NOT NULL,
    github_login        TEXT NOT NULL,
    scopes              TEXT NOT NULL DEFAULT '',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

ALTER TABLE repos DROP COLUMN pat_encrypted;
ALTER TABLE repos DROP COLUMN pat_nonce;
