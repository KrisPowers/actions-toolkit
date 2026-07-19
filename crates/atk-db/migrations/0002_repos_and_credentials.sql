CREATE TABLE repos (
    id                          TEXT PRIMARY KEY,
    owner                       TEXT NOT NULL,
    name                        TEXT NOT NULL,
    default_branch              TEXT NOT NULL DEFAULT 'main',
    pat_encrypted               BLOB NOT NULL,
    pat_nonce                   BLOB NOT NULL,
    webhook_secret_encrypted    BLOB NOT NULL,
    webhook_secret_nonce        BLOB NOT NULL,
    created_by                  TEXT NOT NULL REFERENCES users(id),
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    UNIQUE(owner, name)
);
