-- Agents authenticate the same way buckets do (see buckets.auth_token_hash): a bearer token
-- issued once at join time, only its hash ever persisted. `mtls_fingerprint` named this column
-- for a transport-level identity this subsystem doesn't implement yet (see the security note on
-- atk_rcp::tcp); renaming now, before this migration has shipped in a release, rather than
-- carrying the wrong name forward.

ALTER TABLE agents RENAME COLUMN mtls_fingerprint TO auth_token_hash;
