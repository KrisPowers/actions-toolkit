# maverick

Run CI/CD workflows on your own machine, in isolation, from a terminal UI. Maverick finds the
workflow files in your project, lets you pick a workflow or a single job, and runs it in
**Docker/Podman**, a **virtual machine** of your choice, or its **built-in sandbox** while you
watch every step live. It works fully offline and needs no account or hosted service.

```
 ◆ maverick › workflows › catalog   ~/code/my-app            event push   engine ● Docker
╭ Catalog ─────────────────────────────────────────────────────────────────────────────────╮
│ ▾ CI                                                                               ci.yml│
│   ├ lint                                                                   ubuntu-latest│
│   ├ test                                                               ×3  ubuntu-latest│
│ ▾ Release                                                           global · release.yml │
╰──────────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ move   enter run   e event   r rescan   ⌫ back   q quit
```

## Install

```sh
cargo install --path .
```

This installs two commands: `maverick` and `workflows`.

Works on macOS, Linux and Windows. Engines are optional and detected automatically.

## Usage

```sh
maverick              # settings and the guide
workflows             # your workflow catalog and the history of runs
```

Both work on the project in the current folder; add `-C <folder>` for another one. Everything
else happens in the TUI, with the keyboard:

| Key | Action |
| --- | --- |
| `↑↓` | Move |
| `enter` / `→` | Select or open |
| `⌫` / `←` | Back (leaving the first screen quits) |
| `q` / `ctrl-c` | Quit |

### `maverick`: settings and guide

| Section | What's there |
| --- | --- |
| Guide | How maverick works, its commands, keys and files |
| Engines | Every engine found on this machine. `enter` makes one the default, `t` sets a VM's instance or SSH target |
| Workflow locations | Where workflows are searched. Add folders or files for this project or every project, or remove them |
| Run options | The event runs simulate, and how many jobs run at once |
| Data | Clear this project's run history, or reset your settings |

### `workflows`: Catalog and Runs

A two-option menu:

- **Catalog**: every workflow found for this project and its jobs. `enter` runs the workflow
  (all jobs) or a single job and opens the live run view. `e` changes the simulated event for
  this session.
- **Runs**: past runs of this project, newest first. `enter` reopens a run with all its logs,
  `r` runs it again, `d` deletes it, `x` clears the history.

### Run view

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

### Confirmations and text fields

Actions that can't be undone ask first: deleting a run, clearing history, removing a workflow
location, cancelling a run, resetting settings, and quitting while a run is going. "Cancel" is
selected by default; choose with `← →` and press `enter`.

Some settings are typed in directly: workflow locations (`tab` completes paths), VM targets
and SSH keys, and the number of jobs at once. `enter` saves, `esc` cancels.

## Where workflows are found

Maverick finds the project root from the folder you start it in, then searches:

| Location | Set by |
| --- | --- |
| `.github/workflows/` and `.maverick/workflows/` | Always searched |
| Folders or files listed for this project | Settings › Workflow locations (saved to `.maverick.toml`) |
| Folders or files listed for every project | Settings › Workflow locations (saved to your user config) |

Any `.yml`/`.yaml` file with `jobs:` in an added location is listed; other YAML files there
are ignored.

## Engines

| Engine | Type | How it runs your job |
| --- | --- | --- |
| Docker, Podman | container | One container per job from the `runs-on` image; steps run via `exec` |
| Lima, Multipass | vm | Inside a named instance (`default` / `primary` unless you set one) |
| WSL | vm (Windows) | Inside your default distro, or the one you set |
| Parallels | vm (macOS) | Inside the Linux VM you set |
| VirtualBox, VMware, QEMU, Hyper-V, Tart | vm | Over SSH to the Linux guest you set |
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
on Windows). A project can add `.maverick.toml` at its root, which overrides it. The settings
screen writes both for you; the full format is:

```toml
engine = "docker"          # default engine
event = "push"             # event runs simulate
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

Run history is kept in your data folder (`~/.local/share/maverick/runs` on Linux,
`~/Library/Application Support/maverick/runs` on macOS, `%APPDATA%\maverick\runs` on Windows).

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
workflows -C examples/demo   # a workflow that exercises most features
```

## License

MIT
