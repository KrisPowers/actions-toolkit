# MCP

Reserved for a Model Context Protocol server that lets AI agents drive actions-toolkit directly
(trigger workflow runs, read logs/artifacts, inspect run status) instead of going through the UI or
hand-rolled HTTP calls against the REST API.

Not implemented yet, this is a placeholder path so the server has a home once it's built. It will
likely be its own crate under `crates/` (e.g. `crates/atk-mcp`) exposing tools that call into the
same `core` API the UI already uses, with its own binary target here in `mcp/`.
