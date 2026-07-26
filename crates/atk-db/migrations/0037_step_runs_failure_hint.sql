-- A failed `run:` step whose output matches a known "command not found" pattern (PowerShell,
-- cmd.exe, or a POSIX shell) gets a plain-language diagnosis persisted here: the Shard sandbox's
-- default read-only slice only covers base OS directories, so a toolchain installed under the
-- user profile (cargo, nvm, pyenv, ...) needs its path allowlisted under Settings before a step
-- that invokes it can run. Lets the run detail page surface that directly instead of an operator
-- having to recognize the raw shell error themselves.

ALTER TABLE step_runs ADD COLUMN failure_hint TEXT;
