-- Renames the `buckets` table (one row per job's native sandbox instance) to `job_sandboxes`,
-- freeing up "bucket" to mean the higher-level container for a whole triggering event in a
-- later migration, without the two concepts colliding on the same name.

ALTER TABLE buckets RENAME TO job_sandboxes;

DROP INDEX idx_buckets_reaped;
CREATE INDEX idx_job_sandboxes_reaped ON job_sandboxes (reaped_at, ttl_expires_at);
