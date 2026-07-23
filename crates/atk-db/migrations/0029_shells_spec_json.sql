-- Holds a `ShellRunSpec` for a shell scheduled onto a remote agent, so the agent can fetch it
-- over the API instead of needing filesystem access to wherever the control plane would otherwise
-- have written a local spec file. NULL for a locally-spawned shell, which still gets its spec via
-- a plain temp file (no network hop needed when the shell is a child process on this same host).

ALTER TABLE shells ADD COLUMN spec_json TEXT;
