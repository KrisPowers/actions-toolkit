-- Per-repo webhook tunnels (Decision 1: no repo shares a tunnel with another repo) and a
-- separate dashboard/API remote-access tunnel (Decision 3: never shares a process or port with
-- any webhook tunnel). Also adds installation discovery for org repo sources (Decision 2) and an
-- audit trail for requests that arrive over the dashboard tunnel specifically.

-- One optional row per repo: a repo that has never had a one-click tunnel configured (manual
-- port-forward / pasted "other tunnel" URL) never gets a row at all. Cascades on repo delete so
-- disconnecting a repo cleanly drops its tunnel config with it.
CREATE TABLE repo_tunnels (
    repo_id     TEXT PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,              -- 'cloudflare' | 'tailscale'
    enabled     INTEGER NOT NULL DEFAULT 0, -- persisted "should auto-start on boot" flag
    local_port  INTEGER,                    -- last bound loopback port (best-effort cache only)
    last_url    TEXT,                       -- last known public URL (best-effort cache only)
    updated_at  TEXT NOT NULL
);

-- Singleton row for the instance's own dashboard/API remote-access tunnel. Kept separate from
-- `settings` since it carries a provider choice + cached port/url, not a general instance setting.
CREATE TABLE dashboard_tunnel (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    provider    TEXT NOT NULL DEFAULT 'cloudflare',
    enabled     INTEGER NOT NULL DEFAULT 0,
    local_port  INTEGER,
    last_url    TEXT,
    updated_at  TEXT NOT NULL
);
INSERT INTO dashboard_tunnel (id, enabled, updated_at) VALUES (1, 0, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

-- Which GitHub org/personal-account installations of the App this instance's account-wide
-- connection currently has, so repos can be sourced from any of them (not just the first found).
-- Refreshed wholesale (delete + reinsert) on reconnect or an explicit "refresh installations"
-- action; this is a discovery cache, not a credential.
CREATE TABLE github_installations (
    id              INTEGER PRIMARY KEY, -- GitHub's installation_id
    account_login   TEXT NOT NULL,
    account_type    TEXT NOT NULL,       -- 'User' | 'Organization'
    app_slug        TEXT,
    connected_at    TEXT NOT NULL
);

-- Audit log of requests that arrived over the dashboard tunnel specifically (never LAN/local
-- traffic -- only the hardened tunnel-facing listener writes here). Kept separate from
-- `login_events`: that table's outcome enum and columns are shaped around login attempts, not
-- general per-request auditing.
CREATE TABLE dashboard_tunnel_requests (
    id            TEXT PRIMARY KEY,
    user_id       TEXT REFERENCES users(id) ON DELETE SET NULL,
    ip_address    TEXT,
    method        TEXT NOT NULL,
    path          TEXT NOT NULL,
    status_code   INTEGER NOT NULL,
    rate_limited  INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);
CREATE INDEX idx_dashboard_tunnel_requests_created_at ON dashboard_tunnel_requests(created_at DESC);
CREATE INDEX idx_dashboard_tunnel_requests_user_id ON dashboard_tunnel_requests(user_id);
