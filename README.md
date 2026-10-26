# Actions Toolkit (ATK)

Run your CI/CD workflows on your own machine instead of paying for hosted runner minutes. ATK
finds the workflow files in your project, runs a workflow or a single job with every job in its
own **Docker** container, and lets you watch every step live. It works offline and needs no
account, server or hosted service.

There are three ways to drive it, all over the same runs and the same history:

- **A terminal UI**: run `atk` and pick a workflow.
- **Plain commands**: `atk run ci`, `atk watch`, `atk status`, with `--json` for scripts.
- **AI agents**: `atk mcp` lets Claude, OpenAI or Ollama based agents run, watch and review
  workflows (see [AI agents](docs/ai-agents.md)).

```
 ◆ atk › workflows › catalog   ~/code/my-app                    event push   engine ● Docker
╭ Catalog ─────────────────────────────────────────────────────────────────────────────────╮
│ ▾ CI                                                                               ci.yml│
│   ├ lint                                                                   ubuntu-latest│
│   ├ test                                                               ×3  ubuntu-latest│
│ ▾ Release                                                           global · release.yml │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ move   enter run   e event   r rescan   ⌫ back   q quit
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
brew install https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/Formula/actions-toolkit.rb
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
| `atk mcp` / `atk tools` / `atk call` | The AI agent tools, see [AI agents](docs/ai-agents.md) |

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

- **Catalog**: every workflow found for this project and its jobs. `enter` runs the workflow
  (all jobs) or a single job and opens the live run view. `e` changes the simulated event for
  this session.
- **Runs**: past runs of this project, newest first. `enter` reopens a run with all its logs,
  `r` runs it again, `d` deletes it, `x` clears the history.

| Key | Action |
| --- | --- |
| `↑↓` | Move |
| `enter` / `→` | Select or open |
| `⌫` / `←` | Back (leaving the first screen quits) |
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
| `⌫` | Back (asks to cancel first if still running) |

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

## Development

```sh
cargo test                                  # tests that run jobs need Docker, others skip them
cargo clippy --all-targets -- -D warnings
cargo run -- -C examples/demo               # a workflow that exercises most features
```

## License

[MIT](LICENSE)
