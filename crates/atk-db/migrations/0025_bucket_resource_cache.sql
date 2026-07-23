-- Bucket-scoped shared resource cache: lets sibling shells in the same bucket reuse a resource
-- one of them already generated (e.g. `npm ci`'s node_modules) instead of regenerating it.
-- `status='building'` rows are a lease: exactly one shell wins the race to build a given
-- `cache_key` (see the atomic insert-then-check in resource_cache queries), everyone else polls
-- until it flips to 'ready' or 'failed'. `builder_heartbeat_at` lets the periodic reaper sweep
-- detect a builder that died mid-build and reset the lease so a waiter can retry instead of
-- waiting forever.

CREATE TABLE bucket_resource_cache (
    id                   TEXT PRIMARY KEY,
    bucket_id            TEXT NOT NULL REFERENCES buckets(id) ON DELETE CASCADE,
    cache_key             TEXT NOT NULL,
    status                TEXT NOT NULL,
    path_on_disk           TEXT,
    size_bytes              INTEGER,
    builder_shell_id          TEXT REFERENCES shells(id),
    builder_heartbeat_at        TEXT,
    created_at                    TEXT NOT NULL,
    ready_at                        TEXT,
    failed_at                         TEXT,
    UNIQUE(bucket_id, cache_key)
);

CREATE INDEX idx_bucket_resource_cache_building ON bucket_resource_cache (status, builder_heartbeat_at);
