-- Periodic runtime-resource samples (CPU%, memory, disk I/O, process count) reported by a shell
-- for itself and for each shard it drives. Bucket-level numbers are a computed rollup over their
-- child shells' rows rather than a subject here (a bucket has no OS process of its own to sample).
-- `workflow_run_id` is nullable only because sqlite's ALTER TABLE can't add a NOT NULL column
-- without a default across existing tables in general; every row this feature actually writes
-- always sets it.

CREATE TABLE resource_samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subject_type TEXT NOT NULL,        -- 'shell' | 'shard'
    subject_id TEXT NOT NULL,
    workflow_run_id TEXT REFERENCES workflow_runs(id) ON DELETE CASCADE,
    ts TEXT NOT NULL,
    cpu_percent REAL,
    memory_bytes INTEGER,
    disk_read_bytes INTEGER,
    disk_write_bytes INTEGER,
    process_count INTEGER,
    host_cpu_percent REAL,
    host_memory_percent REAL
);

CREATE INDEX idx_resource_samples_subject ON resource_samples (subject_type, subject_id, ts);
CREATE INDEX idx_resource_samples_run ON resource_samples (workflow_run_id, ts);
