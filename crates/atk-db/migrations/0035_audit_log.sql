-- Per-repo audit trail: every run dispatched, every workflow change, and every repo-level
-- integration action (secrets, webhooks, sync), whether a user clicked something or it happened
-- automatically (a webhook-triggered run, a polled release sync). actor_id/actor_login are both
-- nullable and denormalized like login_events: nullable because plenty of entries here have no
-- human actor at all, denormalized (rather than joining users at read time) so the log still
-- reads sensibly after the acting user is deleted or renames their GitHub account.
CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    repo_id     TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    actor_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    actor_login TEXT,
    action      TEXT NOT NULL,   -- e.g. 'workflow.created', 'run.dispatched', 'secret.deleted'
    target_type TEXT,            -- 'workflow' | 'run' | 'secret' | 'repo'
    target_id   TEXT,
    summary     TEXT NOT NULL,   -- precomputed human-readable line, e.g. 'Created workflow "CI"'
    metadata    TEXT,            -- optional JSON blob for anything not worth its own column
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_audit_log_repo_id_created_at ON audit_log(repo_id, created_at DESC);
