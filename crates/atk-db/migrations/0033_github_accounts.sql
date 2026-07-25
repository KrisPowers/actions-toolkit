-- Retires the local username/password account system in favor of GitHub-identity
-- accounts. Existing local accounts have no reliable mapping to a GitHub identity, so
-- this intentionally drops and rebuilds users/sessions rather than trying to carry them
-- forward: everyone signs back in via GitHub afterward, and whoever does that first
-- becomes the new admin, exactly like a fresh install (see users_queries::count() == 0).

-- repos.created_by and secrets.created_by are NOT NULL REFERENCES users(id) with no
-- ON DELETE action, so on any instance that already has a repo or secret, dropping
-- users below fails with a foreign key constraint violation (SQLite refuses to drop a
-- parent table while a child row still references it). Rebuilding repos/secrets
-- themselves to fix this would DROP those tables too, which implicitly deletes their
-- rows first and, with foreign keys enabled, cascades through every table that
-- references them (workflows, workflow_runs, webhook_events, and beyond) -- the same
-- trap migration 0020 hit. created_by is write-only audit metadata (never read back or
-- joined against users), so instead the column is rebuilt in place via SQLite's
-- add/copy/drop/rename dance, which changes only repos'/secrets' own schema and touches
-- no rows in any other table, to drop the now-unenforceable reference to the
-- about-to-be-rebuilt users table.
ALTER TABLE repos ADD COLUMN created_by_new TEXT NOT NULL DEFAULT '';
UPDATE repos SET created_by_new = created_by;
ALTER TABLE repos DROP COLUMN created_by;
ALTER TABLE repos RENAME COLUMN created_by_new TO created_by;

ALTER TABLE secrets ADD COLUMN created_by_new TEXT NOT NULL DEFAULT '';
UPDATE secrets SET created_by_new = created_by;
ALTER TABLE secrets DROP COLUMN created_by;
ALTER TABLE secrets RENAME COLUMN created_by_new TO created_by;

DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS users;

CREATE TABLE users (
    id              TEXT PRIMARY KEY,
    github_id       INTEGER NOT NULL UNIQUE,
    github_login    TEXT NOT NULL,
    display_name    TEXT,
    avatar_url      TEXT,
    role            TEXT NOT NULL DEFAULT 'member',
    status          TEXT NOT NULL DEFAULT 'pending',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    last_login_at   TEXT
);

CREATE TABLE sessions (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL,
    expires_at      TEXT NOT NULL,
    revoked         INTEGER NOT NULL DEFAULT 0
);

-- Pre-approved GitHub logins that may not have signed in yet; an admin can whitelist
-- someone before their first login, since github_id isn't known until they actually
-- authenticate.
CREATE TABLE github_whitelist (
    github_login    TEXT PRIMARY KEY COLLATE NOCASE,
    added_by        TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE login_events (
    id              TEXT PRIMARY KEY,
    user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
    github_login    TEXT,
    github_id       INTEGER,
    ip_address      TEXT,
    user_agent      TEXT,
    outcome         TEXT NOT NULL, -- 'approved' | 'pending' | 'restricted' | 'denied' | 'rate_limited' | 'failed'
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_login_events_created_at ON login_events(created_at DESC);
CREATE INDEX idx_login_events_user_id ON login_events(user_id);
