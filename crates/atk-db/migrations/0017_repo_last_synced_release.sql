-- Tracks the last release this instance has already reacted to via polling, so a repo that
-- can't receive webhooks (see the webhook-reachability status added earlier) can still fire
-- `on: release` workflows without dispatching the same release twice.

ALTER TABLE repos ADD COLUMN last_synced_release_id INTEGER;
