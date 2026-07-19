# Backend crate layout

The Rust backend is a Cargo workspace, not a single crate. `core` is the binary; everything under
`crates/` is a library crate that `core` depends on.

```
core (bin: actions-toolkit)
 ├── atk-auth      (jwt, password)
 ├── atk-bucket    ─┬── atk-config
 │                  └── atk-db ── atk-crypto
 ├── atk-config
 ├── atk-crypto
 ├── atk-db ── atk-crypto
 ├── atk-github
 └── atk-workflow
```

None of the `crates/atk-*` crates depend on `core`, and none of them depend on each other except
where drawn above. `atk-crypto` and `atk-workflow` have no internal dependencies at all.

## Why the split is where it is

The natural boundary isn't "one crate per existing `src/` subfolder": several modules (`auth`,
`github`, `runner`) are split internally, because part of their code depends on `core::app::AppState`
(the struct holding the DB pool, JWT codec, Docker client, GitHub client cache, etc.) and part
doesn't.

- **`core::auth`** re-exports `atk_auth::{jwt, password}` (pure primitives, no app state) and adds
  its own `handlers` and `middleware` modules (axum extractors that take `State<AppState>`).
- **`core::github`** re-exports `atk_github::{actions, checkout, discovery, hooks, issues, oauth,
  releases, webhook_verify}` (plain GitHub REST calls against a caller-supplied client) and adds
  its own `client` module (builds an authenticated `Octocrab` from `AppState`, including token
  refresh).
- **`core::runner`** stays entirely in `core`: `dispatch`, `executor`, and `scheduler` are the
  actual job-scheduling loop and need `AppState` directly; `docker`, `log_stream`,
  `artifact_capture`, and `workspace` are tightly coupled to that same loop, so splitting them into
  a separate crate would add an indirection without a real boundary behind it.
- **`core::config`** re-exports `atk_config::*` (CLI args, `AppConfig`, zero internal deps) and adds
  `bootstrap()`, which wires together `atk_db::connect`, `atk_crypto::EncryptionKey`, and
  `atk_github::oauth` constants at startup, so it can't live in the dependency-free config crate.

Everywhere a module was split, `core` re-exports the extracted half under its original path
(`crate::db`, `crate::auth::jwt`, `crate::github::oauth`, `crate::bucket`, `crate::workflow`,
`crate::crypto`), so nothing outside of `core`'s own `Cargo.toml` needed to change to make this
split.

## Adding a new crate

Only extract a module if it's genuinely decoupled from `AppState` (check with
`grep -rn AppState core/src/<module>`). If it isn't, it belongs in `core` alongside the other
orchestration code, however large `core/src` gets, rather than forcing an artificial crate
boundary through it.
