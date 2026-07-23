-- The per-job OS isolation unit a shell creates and tears down for each job in its run is a
-- shard: a child of the shell, same way the shell itself is a child of its bucket. Renaming the
-- table (previously renamed once already, from `buckets`, in migration 0021) to match, now that
-- the Bucket -> Shell -> Shard hierarchy is the settled vocabulary for this system.

ALTER TABLE job_sandboxes RENAME TO shards;

DROP INDEX idx_job_sandboxes_reaped;
CREATE INDEX idx_shards_reaped ON shards (reaped_at, ttl_expires_at);
