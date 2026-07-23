-- Per-shell resource-cache hit/miss counters, reported once at shell exit alongside its exit code
-- (see RcpRequest::ReportShellExit). Gives a per-shell "how much did caching actually help this
-- run" number without needing to replay bucket_resource_cache lookups after the fact.

ALTER TABLE shells ADD COLUMN cache_hits INTEGER NOT NULL DEFAULT 0;
ALTER TABLE shells ADD COLUMN cache_misses INTEGER NOT NULL DEFAULT 0;
