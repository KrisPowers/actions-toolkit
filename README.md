# maverick

Run CI/CD workflows on your own machine, in isolation, from a terminal UI. Maverick finds the
workflow files in your project, lets you pick a workflow or a single job, and runs it in
**Docker/Podman**, a **virtual machine** of your choice, or its **built-in sandbox** while you
watch every step live. It works fully offline and needs no account or hosted service.

```
 ◆ maverick ~/code/my-app                                         event push   engine ● Docker
╭ Workflows ───────────────────────────────────────╮╭ Engines ─────────────────────────────────╮
│ ▾ CI                                       ci.yml││ ● Docker ★                 ctr  ready    │
│   ├ lint                            ubuntu-latest││ ● Maverick Native         host  ready    │
│   ├ test                        ×3  ubuntu-latest││ ◐ Lima                     vm   not running│
│ ▾ Release                   global · release.yml ││ ○ Podman                   ctr  not installed│
╰──────────────────────────────────────────────────╯╰──────────────────────────────────────────╯
 ↑↓ move   tab switch panel   enter run   w locations   e event   r rescan   q quit
```

## Install

```sh
cargo install --path .
```

Works on macOS, Linux and Windows. Engines are optional and detected automatically.

## Usage

```sh
maverick            # open the TUI for the current folder
maverick ~/code/app # or for another project
```

That's the whole command line. Everything else happens in the TUI:

1. Pick a workflow (runs all its jobs) or a single job on the left.
2. Pick an engine on the right and press `enter` to make it the default.
3. Press `enter` on the workflow or job to run it and watch it live.

### Dashboard keys

| Key | Action |
| --- | --- |
| `↑↓` / `j k` | Move |
| `tab` / `← →` | Switch between Workflows and Engines |
| `enter` | Run the workflow/job, or choose the engine |
| `w` | Manage workflow locations |
| `t` | Set a VM engine's instance or SSH target |
| `e` | Change the simulated event (`push`, `pull_request`, ...) |
| `r` | Rescan workflows and engines |
| `q` | Quit |

### Run view keys

| Key | Action |
| --- | --- |
| `↑↓` / `j k` | Move within the focused panel |
| `tab` / `← →` | Switch between Jobs, Steps and Logs |
| `enter` (Steps) | Show only that step's logs (again to clear) |
| `a` | Show all logs for the job |
| `pgup` / `pgdn`, `g` / `G` | Scroll logs, jump to top / follow |
| `c` | Cancel the run |
| `q` | Back to the dashboard (cancels first if still running) |

## Where workflows are found

Maverick finds the project root from the folder you start it in, then searches:

| Location | Set by |
| --- | --- |
| `.github/workflows/` and `.maverick/workflows/` | Always searched |
| Folders or files listed for this project | `w`, then `a` (saved to `.maverick.toml`) |
| Folders or files listed globally | `w`, then `g` (saved to your user config, used in every project) |

Any `.yml`/`.yaml` file with `jobs:` in an added location is listed; other YAML files there
are ignored. `tab` completes paths while typing.

## Engines

| Engine | Type | How it runs your job |
| --- | --- | --- |
| Docker, Podman | container | One container per job from the `runs-on` image; steps run via `exec` |
| Lima, Multipass | vm | Inside a named instance (`default` / `primary` unless set with `t`) |
| WSL | vm (Windows) | Inside your default distro, or the one set with `t` |
| Parallels | vm (macOS) | Inside the Linux VM set with `t` |
| VirtualBox, VMware, QEMU, Hyper-V, Tart | vm | Over SSH to the Linux guest set with `t` |
| Maverick Native | built-in | On the host, in a throwaway sandbox folder with a cleaned environment |

Every engine works on a **copy** of your project (respecting `.gitignore`), so runs can't touch
your working files. The native engine needs nothing installed and starts instantly, but it
isolates files and environment variables, not the OS. Use a container or VM for untrusted
workflows.

### Container images

`runs-on` labels map to images: `ubuntu-latest` and `ubuntu-24.04` use `ubuntu:24.04`, and
`ubuntu-22.04` / `ubuntu-20.04` use the matching tag. A job's `container:` always takes
precedence. Map your own in config:

```toml
[images]
ubuntu-latest = "node:20-bookworm"
```

## Configuration

User config lives in your config folder (`~/.config/maverick/config.toml` on Linux,
`~/Library/Application Support/maverick/config.toml` on macOS, `%APPDATA%\maverick\config.toml`
on Windows). A project can add `.maverick.toml` at its root, which overrides it. The TUI writes
both for you; the full format is:

```toml
engine = "docker"          # default engine
parallel = 4               # jobs at once
workflows = ["pipelines"]  # extra workflow locations

[vars]                     # vars.NAME
REGION = "eu"

[images]
ubuntu-latest = "ubuntu:24.04"

[engines.lima]
instance = "ci"

[engines.virtualbox]
ssh = "runner@127.0.0.1:2222"
identity = "~/.ssh/maverick"
```

Secrets go in a `.secrets` file at the project root (`NAME=value` per line, ignored by git) and
are available as `secrets.NAME`. Their values are replaced with `***` in all output.

## Workflow format

Maverick runs workflows written in the widely used `jobs:` / `steps:` YAML format:

- `on` triggers, `env` and `defaults.run` at every level, `if:` with the full expression
  language (`contains`, `startsWith`, `format`, `toJSON`, `fromJSON`, `success()`,
  `failure()`, `always()`, `cancelled()`, ...)
- `needs` with outputs and results, parallel jobs, `strategy.matrix` with `include`/`exclude`,
  `fail-fast`
- `run` steps with `bash`, `sh`, `pwsh`, `powershell`, `cmd`, `python` and custom shells;
  `working-directory`, `continue-on-error`, `timeout-minutes`
- Step files: append `NAME=value` to `$MAVERICK_ENV` (env for later steps), `$MAVERICK_OUTPUT`
  (step outputs) or a folder to `$MAVERICK_PATH`. `$GITHUB_ENV`, `$GITHUB_OUTPUT` and
  `$GITHUB_PATH` are aliases, so existing files work unchanged.
- Log commands: `::error::`, `::warning::`, `::group::`, `::add-mask::`
- Local actions (`uses: ./path`), composite and Node. `owner/repo@ref` actions are never
  downloaded: put a copy in `.maverick/actions/owner/repo` (project) or in your config folder
  under `maverick/actions/owner/repo` (global), otherwise the step is skipped with a warning.

Not yet supported: Docker-based actions, `services:`, reusable workflows (`jobs.<id>.uses`),
dynamic matrices, caching and artifacts (these steps are skipped with a warning).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
maverick examples/demo   # a workflow that exercises most features
```

## License

MIT
