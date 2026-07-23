-- Links a job's sandbox back to the shell process that owns it, so cleanup and diagnostics can
-- walk from a job sandbox to the shell (and from there, the bucket) it belongs to.

ALTER TABLE job_sandboxes ADD COLUMN shell_id TEXT REFERENCES shells(id);
