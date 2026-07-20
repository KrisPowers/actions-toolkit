-- Bucket's startup capability probe and TTL reaper were wired into main.rs, but there was no way
-- to configure or observe them. Adds the settings columns; wiring each one through to actual
-- bucket creation is tracked incrementally (bucket_default_ttl_seconds first).

ALTER TABLE settings ADD COLUMN bucket_default_ttl_seconds INTEGER NOT NULL DEFAULT 21600;
ALTER TABLE settings ADD COLUMN bucket_cpu_limit_millis INTEGER;
ALTER TABLE settings ADD COLUMN bucket_memory_limit_mb INTEGER;
ALTER TABLE settings ADD COLUMN bucket_host_mounts_json TEXT NOT NULL DEFAULT '[]';
