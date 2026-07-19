-- Tracks the GitHub-side webhook hook ID for a connected repo, so disconnecting can remove the
-- real webhook instead of just the local row. Nullable: a repo connected before this migration
-- (with a manually-created webhook) simply has no hook to automatically manage until reconnected.
ALTER TABLE repos ADD COLUMN github_hook_id INTEGER;
