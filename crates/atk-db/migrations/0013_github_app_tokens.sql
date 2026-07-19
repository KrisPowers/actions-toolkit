-- Extends the single-row github_token table to hold GitHub App user-to-server tokens
-- (access + refresh token, expiry, installation) alongside the legacy PAT it already stores.
-- Every new column is nullable (or has a default), so the existing PAT row stays valid without
-- a backfill: token_type defaults to 'pat', needs_reconnect defaults to false.

ALTER TABLE github_token ADD COLUMN token_type TEXT NOT NULL DEFAULT 'pat';
ALTER TABLE github_token ADD COLUMN refresh_token_encrypted BLOB;
ALTER TABLE github_token ADD COLUMN refresh_token_nonce BLOB;
ALTER TABLE github_token ADD COLUMN expires_at TEXT;
ALTER TABLE github_token ADD COLUMN installation_id INTEGER;
ALTER TABLE github_token ADD COLUMN needs_reconnect INTEGER NOT NULL DEFAULT 0;
