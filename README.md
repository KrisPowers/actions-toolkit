# Actions Toolkit (ATK)

Run your CI/CD workflows on your own machine instead of paying for hosted runner minutes. ATK
finds the workflow files in your project, runs a workflow or a single job with every job in its
own **Docker** container, and lets you watch every step live. It works offline and needs no
account, server or hosted service.

There are three ways to drive it, all over the same runs and the same history:

- **A terminal UI**: run `atk` and pick a workflow.
- **Plain commands**: `atk run ci`, `atk watch`, `atk status`, with `--json` for scripts.
- **AI agents**: `atk mcp` lets Claude, OpenAI or Ollama based agents run, watch and review
  workflows (see [AI agents](#ai-agents)).

```
 ◆ atk › workflows › catalog   ~/code/my-app                        event push   engine ● Docker
╭ Catalog ─────────────────────────────────────╮╭ Details ─────────────────────────────────────╮
│ ▾ CI                                   ci.yml││ test                                         │
│   ▾ lint                        ubuntu-latest││ job test in CI                               │
│     · Run actions/checkout@v4                ││                                              │
│     · Lint                                   ││ runs-on   ubuntu-latest                      │
│   ▾ test                    ×3  ubuntu-latest││ needs     lint                               │
│     · Run actions/checkout@v4                ││ matrix    3                                  │
│     · Test                                   ││ steps     2                                  │
│ ▸ Release                global · release.yml││                                              │
│                                              ││ r: run just this job as `push`               │
╰──────────────────────────────────────────────╯╰──────────────────────────────────────────────╯
 ↑↓ move   enter collapse   c collapse all   r run   e event   f5 rescan   esc back   q quit
```

## Install

ATK needs [Docker](https://docs.docker.com/engine/install/) (Docker Engine or Docker Desktop)
installed and running. Then:

```sh
curl -fsSL https://atk.krispowers.dev | sh
```

On Windows (PowerShell):

```powershell
irm https://atk.krispowers.dev | iex
```

Or via Homebrew, or from source with a Rust toolchain:

```sh
brew tap krispowers/atk https://github.com/KrisPowers/actions-toolkit
brew install atk
cargo install --path .
```

Either way you get one command, `atk`. Check your setup with `atk doctor`.

## Usage

Every command works on the project in the current folder; add `-C <folder>` for another one.

| Command | What it does |
| --- | --- |
| `atk` | The terminal UI: your workflow catalog and the history of runs |
| `atk settings` | The terminal UI's settings and guide |
| `atk list` | This project's workflows and their jobs |
| `atk validate [workflow]` | Check workflow files for errors without running them |
| `atk run <workflow>` | Run a workflow in the foreground, streaming its logs |
| `atk run <workflow> --detach` | Start it in the background and print its run id |
| `atk runs` | Past and running runs, newest first |
| `atk status [id]` | A run's jobs and steps, and the output of whatever failed |
| `atk logs [id]` | A run's logs (`--job`, `--step`, `--tail N`) |
| `atk watch [id]` | Follow a run until it ends; exits with the run's result |
| `atk cancel [id]` | Stop a run that is still going |
| `atk rerun [id]` | Run a past run's workflow again |
| `atk delete <id>` / `atk clear --yes` | Remove one run, or every finished run, from the history |
| `atk config` | Show settings; `atk config set event pull_request`, `atk config unset ...` |
| `atk locations` | Where workflows are searched; `atk locations add pipelines` |
| `atk doctor` | Whether Docker is ready, and where ATK keeps its files |
| `atk mcp` / `atk tools` / `atk call` | The AI agent tools, see [AI agents](#ai-agents) |

A `<workflow>` is its path, its file name (with or without `.yml`) or its `name:`. A run id can
be the full id, a unique start of one, or `last`, which is also the default.

`atk run` options:

| Option | Meaning |
| --- | --- |
| `-j, --job <id>` | Run only this job (all its matrix variants), treating its `needs` as done |
| `-e, --event <name>` | Event to simulate: `push`, `pull_request`, `workflow_dispatch`, ... |
| `--input NAME=VALUE` | A `workflow_dispatch` input |
| `--var NAME=VALUE` | A value for `vars.NAME` |
| `--image LABEL=IMAGE` | The image for jobs with `runs-on: LABEL` |
| `--pull` | Always pull images |
| `--bind` | Mount the project into containers instead of copying it |
| `-d, --detach` | Run in the background |
| `--json` | Print events as JSON lines |

Exit codes: `0` success, `1` failure (or an error), `130` cancelled. `atk watch` exits the same
way, so a script can start a run with `--detach` and wait for it later.

Runs are recorded while they go, wherever they were started: a run started in the TUI can be
followed with `atk watch` in another terminal, and `atk cancel` stops a run the TUI or an agent
started.

### The terminal UI

`atk` opens a two-option menu:

- **Catalog**: every workflow found for this project, its jobs and their steps, with the
  selection's details beside the list. `enter` (or `← →`) collapses and expands a workflow or
  job, `c` collapses or expands everything, and `r` runs the selected workflow (all jobs) or job
  (a step runs its job) and opens the live run view. `e` changes the simulated event for this
  session and `f5` rescans for workflow files.
- **Runs**: past runs of this project, newest first. `enter` reopens a run with all its logs,
  `r` runs it again, `d` deletes it, `x` clears the history.

| Key | Action |
| --- | --- |
| `↑↓` | Move |
| `enter` / `→` | Select or open |
| `esc` / `backspace` | Back (leaving the first screen quits) |
| `q` / `ctrl-c` | Quit |

In the run view:

| Key | Action |
| --- | --- |
| `↑↓` / `j k` | Move within the focused panel |
| `tab` / `← →` | Switch between Jobs, Steps and Logs |
| `enter` (Steps) | Show only that step's logs (again to clear) |
| `a` | Show all logs for the job |
| `pgup` / `pgdn`, `g` / `G` | Scroll logs, jump to top / follow |
| `c` | Cancel the run |
| `r` | Run it again (once finished) |
| `esc` / `backspace` | Back (asks to cancel first if still running) |

`atk settings` has the guide, Docker's status, workflow locations, run options (the default
event and how many jobs run at once) and history cleanup. Actions that can't be undone ask
first; "Cancel" is selected by default.

## Where workflows are found

ATK finds the project root from the folder you start it in, then searches:

| Location | Set by |
| --- | --- |
| `.github/workflows/` and `.atk/workflows/` | Always searched |
| Folders or files listed for this project | `atk locations add` or settings (saved to `.atk.toml`) |
| Folders or files listed for every project | `atk locations add --global` or settings (your user config) |

Any `.yml`/`.yaml` file with `jobs:` in an added location is listed; other YAML files there are
ignored.

## How jobs run

Each job gets one container, started from its `runs-on` image and kept alive for the whole job,
with every step run inside it via `docker exec`. The container works on a **copy** of your
project (respecting `.gitignore`), so runs can't touch your working files unless you pass
`--bind`. Containers are labelled `dev.atk.run=1` and removed when the job ends.

`runs-on` labels map to images: `ubuntu-latest` and `ubuntu-24.04` use `ubuntu:24.04`, and
`ubuntu-22.04` / `ubuntu-20.04` use the matching tag. A job's `container:` always takes
precedence. Map your own with `atk config set image.ubuntu-latest node:20-bookworm`, or in
config:

```toml
[images]
ubuntu-latest = "node:20-bookworm"
```

## Configuration

User config lives in your config folder (`~/.config/atk/config.toml` on Linux,
`~/Library/Application Support/atk/config.toml` on macOS, `%APPDATA%\atk\config.toml` on
Windows). A project can add `.atk.toml` at its root, which overrides it. `atk config set` and
the settings screen write both for you; the full format is:

```toml
event = "push"             # event runs simulate
parallel = 4               # jobs at once
workflows = ["pipelines"]  # extra workflow locations

[vars]                     # vars.NAME
REGION = "eu"

[images]
ubuntu-latest = "ubuntu:24.04"
```

Run history is kept in your data folder (`~/.local/share/atk/runs` on Linux,
`~/Library/Application Support/atk/runs` on macOS, `%APPDATA%\atk\runs` on Windows).

Secrets go in a `.secrets` file at the project root (`NAME=value` per line, keep it out of git)
and are available as `secrets.NAME`. Their values are replaced with `***` in all output.

## Workflow format

ATK runs workflows written in the widely used `jobs:` / `steps:` YAML format:

- `on` triggers, `env` and `defaults.run` at every level, `if:` with the full expression
  language (`contains`, `startsWith`, `format`, `toJSON`, `fromJSON`, `success()`,
  `failure()`, `always()`, `cancelled()`, ...)
- `needs` with outputs and results, parallel jobs, `strategy.matrix` with `include`/`exclude`,
  `fail-fast`
- `run` steps with `bash`, `sh`, `pwsh`, `powershell`, `cmd`, `python` and custom shells;
  `working-directory`, `continue-on-error`, `timeout-minutes`
- Step files: append `NAME=value` to `$ATK_ENV` (env for later steps), `$ATK_OUTPUT` (step
  outputs) or a folder to `$ATK_PATH`. `$GITHUB_ENV`, `$GITHUB_OUTPUT` and `$GITHUB_PATH` are
  aliases, so existing files work unchanged.
- Log commands: `::error::`, `::warning::`, `::group::`, `::add-mask::`
- Local actions (`uses: ./path`), composite and Node. `owner/repo@ref` actions are never
  downloaded: put a copy in `.atk/actions/owner/repo` (project) or in your config folder under
  `atk/actions/owner/repo` (global), otherwise the step is skipped with a warning.

Not yet supported: Docker-based actions, `services:`, reusable workflows (`jobs.<id>.uses`),
dynamic matrices, caching and artifacts (these steps are skipped with a warning).

## AI agents

An AI model running on your machine, whether local through Ollama or in the cloud through
Anthropic or OpenAI, can run your workflows with ATK, watch them, and review what failed. A
model can't drive a terminal UI, so everything the TUI does is also available as plain
commands and as a set of tools.

There are three ways in, all backed by the same tools and the same run history:

| Way in | Use it when |
| --- | --- |
| `atk mcp` | The agent supports the Model Context Protocol (Claude Code, Claude Desktop, Codex, the OpenAI Agents SDK, MCP-capable Ollama front ends) |
| `atk tools` + `atk call` | You write your own agent loop against a model API (Ollama, OpenAI, Anthropic) |
| `atk <command> --json` | The agent can already run shell commands |

Runs started by an agent show up in `atk runs` and the TUI like any other, and you can follow
one with `atk watch` while the agent works.

### The tools

| Tool | What it does |
| --- | --- |
| `list_workflows` | The project's workflows, their triggers and jobs |
| `validate_workflows` | Parse and plan workflow files without running them |
| `run_workflow` | Start a workflow (or one `job`) in the background; returns the run id |
| `wait_for_run` | Block until a run ends or `timeout_seconds` pass; returns its review and `done` |
| `get_run` | A run's review: outcome, jobs, steps, and each failed step's errors and last output |
| `get_logs` | A page of log lines, optionally for one job and step; pass `next_offset` back to read on |
| `list_runs` | The project's runs, newest first |
| `cancel_run` | Stop a running run |
| `rerun` | Start a past run's workflow again |
| `docker_status` | Whether Docker is ready |

Every tool also takes an optional `project` folder; without it, the folder ATK was started in
is used. Run ids can be `last`. The usual loop is `run_workflow`, then `wait_for_run` until
`done` is true, then `get_run` to review; fix the code, `validate_workflows`, and `rerun`.

### MCP clients

`atk mcp` speaks MCP over stdio. Start it from the project folder, or pass `-C <folder>`.

Claude Code, from the project folder:

```sh
claude mcp add atk -- atk mcp
```

Claude Desktop, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "atk": { "command": "atk", "args": ["-C", "/path/to/project", "mcp"] }
  }
}
```

Codex, in `~/.codex/config.toml`:

```toml
[mcp_servers.atk]
command = "atk"
args = ["mcp"]
```

The OpenAI Agents SDK (Python):

```python
from agents.mcp import MCPServerStdio

atk = MCPServerStdio(params={"command": "atk", "args": ["-C", "/path/to/project", "mcp"]})
```

### Your own agent loop

`atk tools` prints the tool definitions in the shape a model API expects, and `atk call` runs
one, printing its JSON result (or `{"error": ...}` with exit code 1):

```sh
atk tools --format openai      # OpenAI Chat Completions, and Ollama's /api/chat
atk tools --format anthropic   # Anthropic Messages API
atk tools --format mcp         # MCP tools/list entries
atk call run_workflow '{"workflow": "ci", "wait_seconds": 300}'
```

A minimal loop with Ollama:

```python
import json, subprocess, ollama

tools = json.loads(subprocess.check_output(["atk", "tools", "--format", "openai"]))
messages = [{"role": "user", "content": "Run the CI workflow and tell me why it fails."}]
while True:
    reply = ollama.chat(model="qwen3", messages=messages, tools=tools).message
    messages.append(reply)
    if not reply.tool_calls:
        break
    for call in reply.tool_calls:
        args = json.dumps(call.function.arguments)
        result = subprocess.run(["atk", "call", call.function.name, args],
                                capture_output=True, text=True).stdout
        messages.append({"role": "tool", "content": result, "tool_name": call.function.name})
print(reply.content)
```

The same works with OpenAI or Anthropic: send the definitions from `atk tools`, and answer
each tool call with the output of `atk call`.

### Agents with a shell

An agent that can run commands needs nothing extra. Every command takes `--json`:

```sh
atk list --json
atk run ci --detach --json          # {"id": "...", "outcome": "running"}
atk watch last --json               # JSON lines while it runs, then the review; exits 0/1/130
atk status last --json              # the review: jobs, steps, failures
atk logs last --job test --step "Run tests" --tail 50 --json
```

## Development

```sh
cargo test                                  # tests that run jobs need Docker, others skip them
cargo clippy --all-targets -- -D warnings
cargo run -- -C tests/fixtures/demo         # a workflow that exercises most features
```

`tests/release.rs` drives the built `atk` binary end to end (commands, `--json` output, MCP,
Docker runs, failure reviews, detached runs). It runs on every PR, and every release runs it
first: a tag that fails it publishes nothing.

## Releasing

Pushing a `v*` tag runs the release workflow: the release tests first, then `atk` binaries for
macOS (arm64, x86_64), Linux (x86_64) and Windows (x86_64), attached to a GitHub Release with
`.sha256` checksums. Afterwards, `scripts/bump-formula.sh <version>` updates the Homebrew formula,
`Formula/atk.rb`. This repo is the tap.

`atk.krispowers.dev` is a set of Cloudflare Redirect Rules (no Worker or hosting): PowerShell
gets `install.ps1`, curl and wget get `install.sh`, browsers get this repo, all from `main`.

## License

[MIT](LICENSE)
