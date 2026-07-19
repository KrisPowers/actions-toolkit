# actions-toolkit

A self-hosted, local alternative to GitHub Actions. actions-toolkit runs your CI/CD workflows
**on your own machine** instead of GitHub-hosted runners, so you stop paying for Actions minutes
while keeping a workflow-file-driven, trigger-based pipeline you already know.

- **Rust backend** (axum + SQLite + Docker) serves a REST/WebSocket API and executes workflow
  jobs as Docker containers on the host, mirroring GitHub Actions' execution model.
- **React/TypeScript UI** (served by the same binary) gives you configuration, live logs, run
  history, and analytics, plus GitHub issue/PR/release management.
- **Two ways to author workflows**: a full YAML code editor (Monaco), or a drag-and-drop visual
  builder (React Flow) for triggers, jobs, steps, and conditions. Both edit the same underlying
  workflow definition.

## How it works

1. On first run, a setup wizard walks you through creating an admin account, then a "Connect
   GitHub" button that sends you to GitHub to authorize the shared actions-toolkit App. No token
   to generate or paste, and nothing is ever written to an env file.
2. Pick which repos to connect from the App installation's accessible-repos list (or extend the
   installation from GitHub's own settings if a repo you want isn't listed yet).
3. Connecting a repo automatically creates its webhook on GitHub, there's no manual payload
   URL/secret step. If this server isn't publicly reachable, see "Exposing your webhook" below for
   what it needs to be reachable at.
4. Define workflows (`on:` triggers, `jobs:`, `steps:`) either as YAML or visually.
5. When a matching event arrives (or you click "Run now"), actions-toolkit checks out your repo,
   spins up a Docker container per job, runs each step, streams logs live to the UI, and captures
   any declared artifacts, all on your own hardware.

One GitHub connection covers every repo the App installation grants access to; there's no
per-repo credential to manage, and the underlying token is never entered, displayed, or stored as
an environment variable.

If you're upgrading an existing install that was still using the old personal-access-token flow,
Settings will show a persistent "Reconnect" banner after upgrading. Your existing token keeps
working exactly as before until you click it; reconnecting swaps it for an App connection with no
other downtime.

## Install

The backend embeds the built UI into a single binary, so installing gets you both:

```bash
curl -fsSL https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.sh | sh
```

On Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.ps1 | iex
```

Or via Homebrew (macOS and Linux):

```bash
brew install https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/Formula/actions-toolkit.rb
```

Either way, the installer also runs `actions-toolkit init` once, which creates the data directory
(an OS-standard per-user location, e.g. `~/.local/share/actions-toolkit`) and initializes its
SQLite database with default settings, before you ever run `start`. Then you get a single
`actions-toolkit` command:

```bash
actions-toolkit start   # or: actions-toolkit listen
```

This starts the backend API and serves the UI from the same process. By default it listens on
`:7890`; if that port is already taken, it automatically tries the next few ports up and logs
whichever one it actually bound to, so a busy default port won't stop it from starting. Pass
`--port <n>` (or `--bind-addr <addr>`) to change it; the value is saved to the database, so a
later plain `start` remembers it.

No prebuilt binary for your OS/architecture yet? Build from source, see below.

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) 20+ and npm
- [Docker](https://www.docker.com/) running locally. This is what actually executes workflow
  jobs. The server starts without it, but dispatching a workflow will fail until Docker is
  reachable.
- A GitHub account that can install the shared actions-toolkit App (see below) on whichever repos
  you want to connect. No token to generate ahead of time.

## GitHub App

actions-toolkit authenticates to GitHub through a single shared GitHub App,
[`actionstoolkit`](https://github.com/settings/apps/actionstoolkit), owned by the project
maintainer. Its client ID is public and compiled into the binary (see `.env.example` and
`crates/atk-config/src/lib.rs`), so nothing needs registering per install; every instance authorizes through the
same App via the "Connect GitHub" button in Settings, and each user gets their own token scoped to
exactly what they personally approve, an install of the App on the repos they choose. Override
`GITHUB_APP_CLIENT_ID` (and register your own App) only if you're running a fork you want
authenticating independently of the shared one.

## Development

```bash
# Backend (terminal 1), serves the API on :7890 (or the next free port), run from the repo root
cargo run -- start

# UI (terminal 2), Vite dev server on :5173, proxies /api and /webhooks to :7890
cd ui
npm install
npm run dev
```

Open `http://localhost:5173`. On first run the setup wizard walks you through creating an admin
account and connecting your GitHub token.

## Production build

The backend embeds the UI's built assets into a single binary via `rust-embed`, so the UI must
be built first:

```bash
cd ui
npm install
npm run build

cd ..
cargo build --release
./target/release/actions-toolkit init    # creates the data dir + database, safe to skip (start does it too)
./target/release/actions-toolkit start
```

Then open `http://<host>:7890`. Port and bind address are set with `start --port`/`--bind-addr`
(persisted to the database for next time); bind address, Docker host override, and max concurrent
jobs can also be changed from the Settings page in the UI. `.env.example` covers the remaining
advanced overrides: a custom data directory, and recovery of the JWT/encryption secrets.

## Exposing your webhook

GitHub needs to reach this server to deliver push/PR/release events. Connecting a repo creates its
webhook automatically, pointed at this instance's own address, so this only matters if that
address isn't reachable from the internet:

- Use a tunnel such as `ngrok http 7890` or `cloudflared tunnel --url http://localhost:7890`
  *before* connecting the repo, so the webhook actions-toolkit creates points at a reachable URL
  from the start.
- Or run actions-toolkit on a host that's already reachable on your network/VPN.

There's no manual payload URL or secret to copy anywhere, connecting the repo is the whole step.

## Layout

```
core/                  Binary crate: axum HTTP/WebSocket API, app state, auth handlers,
                        Docker/Bucket job orchestration, GitHub client glue. Depends on
                        every crate below.
crates/atk-crypto/      Encryption-at-rest primitives (PATs, webhook secrets)
crates/atk-auth/        JWT signing, password hashing
crates/atk-config/      CLI args and resolved on-disk config, no other internal deps
crates/atk-db/          SQLite pool, models, and the query layer (sqlx), plus migrations/
crates/atk-workflow/    Workflow YAML parsing, expression eval, trigger matching, validation
crates/atk-github/      GitHub REST API surface: checkout, releases, issues, webhooks, OAuth
crates/atk-bucket/      Native per-step sandbox (Linux namespaces/cgroups, Windows AppContainer)
ui/                     React + TypeScript UI (Vite, Tailwind): dashboard, repo/workflow
                        management, dual-mode workflow editor (Monaco + React Flow), live
                        logs, analytics
mcp/                    MCP server exposing actions-toolkit to AI agents (see mcp/README.md)
docs/                   Additional guides beyond this README
install.sh, install.ps1,   cURL/PowerShell installers, Homebrew formula, and the script that
Formula/, scripts/         refreshes the formula's checksums after a release
```

`core` depends on every crate under `crates/`, never the other way around; each `crates/atk-*`
crate is a focused, independently-testable slice with no dependency on app state. A crate only
exists if its code was already decoupled from `core::app::AppState` in practice: modules that
still need the live app state (HTTP handlers, the Docker/Bucket dispatch loop, the GitHub client's
token-refresh glue) stay in `core`, re-exported under their old module paths (`crate::db`,
`crate::auth::jwt`, etc.) so callers don't need to know which piece moved where. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full dependency graph.

The UI gets its own `ui/` directory since it's a separate build toolchain (npm/Vite) that produces
static assets the backend embeds.

Workflow YAML is a scoped-down, GitHub-Actions-flavored syntax: `on:` triggers (`push`,
`pull_request`, `release`, `workflow_dispatch`), `jobs:` with a `container:` image and `needs:`
dependencies, and `steps:` with `run:` (shell) or `uses: docker://image` (container action). Job
containers stay alive for the duration of the job so multiple steps can `exec` into them
sequentially, just like GitHub's own runners.

## Known limitations

- **One GitHub connection for the whole instance.** There's no per-repo or per-org credential;
  every connected repo authenticates through the same App connection. Disconnecting it in Settings
  stops workflow dispatch, webhook processing, and issue/PR/release actions for every connected
  repo until it's reconnected.
- The accessible-repos picker lists up to a few hundred repos (a handful of paginated requests);
  very large orgs may not see their entire repo list there, but can still connect a repo by exact
  owner/name via the manual fallback.
- **Docker is required** to execute anything; there's no non-container execution mode.
- The visual builder and the YAML editor share one canonical model, but the backend regenerates
  YAML on every visual-builder save, so hand-written comments and formatting are not preserved
  once you save from visual mode.
- The `if:` expression support is intentionally minimal (`==`, `!=`, `&&`, `||`, `contains()`,
  `always()`/`success()`/`failure()`, and `needs.<job>.result`/`github.event_name` lookups), not
  the full GitHub Actions expression language.
- All jobs in a run currently share one checked-out workspace (keyed by run, not by job), so
  files one job writes are visible to jobs that run after it even without declaring
  `download_artifacts`. Declaring artifacts is still the explicit, portable way to pass files
  between jobs. Tracked in issue #4.
- There's no polling fallback for hosts that can't receive webhooks at all yet; a tunnel (ngrok/
  cloudflared) is currently the only way to reach a non-public host. Tracked in issue #2.

## Releasing

Pushing a `v*` tag (e.g. `v0.2.0`) runs `.github/workflows/release.yml`, which builds the UI,
compiles release binaries for macOS (arm64 and x86_64), Linux (x86_64), and Windows (x86_64), and
attaches them (with `.sha256` checksums; Windows ships as `.zip`, the rest as `.tar.gz`) to a
GitHub Release. `install.sh`/`install.ps1` always download the `latest` release unless
`ACTIONS_TOOLKIT_VERSION` is set.

After a release finishes, refresh the Homebrew formula's pinned version and checksums:

```bash
scripts/bump-formula.sh 0.2.0
```

Review the diff and commit `Formula/actions-toolkit.rb`.

## License

[MIT](LICENSE)
