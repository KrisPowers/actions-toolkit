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

1. Connect a GitHub repo with a personal access token.
2. Point a GitHub webhook at this server (or tunnel it, see below) so push/PR/release events
   reach it.
3. Define workflows (`on:` triggers, `jobs:`, `steps:`) either as YAML or visually.
4. When a matching event arrives (or you click "Run now"), actions-toolkit checks out your repo,
   spins up a Docker container per job, runs each step, streams logs live to the UI, and captures
   any declared artifacts, all on your own hardware.

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) 20+ and npm
- [Docker](https://www.docker.com/) running locally. This is what actually executes workflow
  jobs. The server starts without it, but dispatching a workflow will fail until Docker is
  reachable.
- A GitHub personal access token with repo scope, for any repo you want to connect

## Development

```bash
# Backend (terminal 1), serves the API on :7890, run from the repo root
cargo run

# UI (terminal 2), Vite dev server on :5173, proxies /api and /webhooks to :7890
cd ui
npm install
npm run dev
```

Open `http://localhost:5173`. On first run you'll be asked to create an admin account.

## Production build

The backend embeds the UI's built assets into a single binary via `rust-embed`, so the UI must
be built first:

```bash
cd ui
npm install
npm run build

cd ..
cargo build --release
./target/release/actions-toolkit-backend
```

Then open `http://<host>:7890`. Configuration is via environment variables or CLI flags, see
`.env.example` for the full list (port, data directory, Docker host override, JWT/encryption
secrets, max concurrent jobs).

## Exposing your webhook

GitHub needs to reach this server to deliver push/PR/release events. If the machine running
actions-toolkit isn't publicly reachable:

- Use a tunnel such as `ngrok http 7890` or `cloudflared tunnel --url http://localhost:7890` and
  register the tunnel's HTTPS URL as the webhook payload URL.
- Or run actions-toolkit on a host that's already reachable on your network/VPN and point the
  webhook there directly.

When you connect a repo in the UI, it generates a per-repo webhook secret and shows you the exact
payload URL and setup steps for GitHub's Settings > Webhooks page.

## Layout

```
Cargo.toml, src/, migrations/, build.rs   Rust backend (axum): REST + WebSocket API, SQLite via
                                           sqlx, Docker execution via bollard, GitHub REST via
                                           octocrab, workflow YAML parsing/validation/scheduling
ui/                                        React + TypeScript UI (Vite, Tailwind): dashboard,
                                           repo/workflow management, dual-mode workflow editor
                                           (Monaco + React Flow), live logs, analytics
```

The backend lives at the repo root rather than in its own subdirectory since it's the primary
artifact this project ships; the UI gets its own `ui/` directory since it's a separate build
toolchain (npm/Vite) that produces static assets the backend embeds.

Workflow YAML is a scoped-down, GitHub-Actions-flavored syntax: `on:` triggers (`push`,
`pull_request`, `release`, `workflow_dispatch`), `jobs:` with a `container:` image and `needs:`
dependencies, and `steps:` with `run:` (shell) or `uses: docker://image` (container action). Job
containers stay alive for the duration of the job so multiple steps can `exec` into them
sequentially, just like GitHub's own runners.

## Known limitations

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

## License

[MIT](LICENSE)
