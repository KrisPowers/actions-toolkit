-- Retires the local username/password account system in favor of GitHub-identity
-- accounts. Existing local accounts have no reliable mapping to a GitHub identity, so
-- this intentionally drops and rebuilds users/sessions rather than trying to carry them
-- forward: everyone signs back in via GitHub afterward, and whoever does that first
-- becomes the new admin, exactly like a fresh install (see users_queries::count() == 0).

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
