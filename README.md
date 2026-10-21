# maverick

Run your CI/CD workflows locally, in isolation, from a single command. Maverick reads your
GitHub Actions workflows and runs them on **Docker/Podman**, a **virtual machine** of your
choice, or its **built-in engine**, with a fast terminal UI to watch everything happen.

```
 ◆ maverick ~/code/my-app                                         event push   engine ● Docker
╭ Workflows ───────────────────────────────────────╮╭ Engines ─────────────────────────────────╮
│ ▾ CI                                       ci.yml││ ● Docker ★                 ctr  ready    │
│   ├ lint                            ubuntu-latest││ ● Maverick Native         host  ready    │
│   ├ test                        ×3  ubuntu-latest││ ◐ Lima                     vm   not running│
│ ▾ Release                             release.yml││ ○ Podman                   ctr  not installed│
╰──────────────────────────────────────────────────╯╰──────────────────────────────────────────╯
 ↑↓ move   tab switch panel   enter run   e event   r rescan   q quit
```

## Install

```sh
cargo install --path .
```

Works on macOS, Linux and Windows. The only hard requirement is the binary itself; engines are
optional and detected automatically.

## Quick start

```sh
maverick                      # dashboard: pick a workflow/job and an engine, press enter
maverick run                  # run every workflow triggered by `push`
maverick run ci -j test       # run one job (all of its matrix variants)
maverick run -e pull_request  # simulate a different event
maverick list                 # workflows, jobs, matrices and dependencies
maverick engines              # what's installed and usable on this machine
```

`maverick run` shows the live run view in a terminal and plain, prefixed logs everywhere else
(pipes, CI, `--plain`). The exit code is non-zero if any job fails.

### Run view keys

| Key | Action |
| --- | --- |
| `↑↓` / `j k` | Move within the focused panel |
| `tab` / `← →` | Switch between Jobs, Steps and Logs |
| `enter` (Steps) | Show only that step's logs (again to clear) |
| `a` | Show all logs for the job |
| `pgup` / `pgdn`, `g` / `G` | Scroll logs, jump to top / follow |
| `c` | Cancel the run |
| `q` | Back (cancels first if still running) |

## Engines

| Engine | Type | How it runs your job |
| --- | --- | --- |
| Docker, Podman | container | One container per job from the `runs-on` image; steps run via `exec` |
| Lima, Multipass | vm | Inside a named instance (`default` / `primary` unless configured) |
| WSL | vm (Windows) | Inside your default (or configured) distro |
| Parallels | vm (macOS) | Inside a named Linux VM via `prlctl exec` |
| VirtualBox, VMware, QEMU, Hyper-V, Tart | vm | Over SSH to a Linux guest you point it at |
| Maverick Native | built-in | On the host, in a throwaway sandbox directory with a scrubbed environment |

Every engine works on a **copy** of your project (respecting `.gitignore`), so runs can't touch
your working tree. Pass `--bind` to use the project in place instead (Docker/native only).

The native engine needs nothing installed and starts instantly, but it isolates files and
environment variables, not the OS. Use a container or VM for untrusted workflows.

### Choosing and configuring engines

```sh
maverick config set-engine docker                      # default engine
maverick config set-target lima my-instance            # VM instance name
maverick config set-target qemu runner@127.0.0.1:2222 --identity ~/.ssh/id_ed25519
maverick run --engine native                           # one-off override
```

Without a default, maverick uses Docker or Podman if one is running, else the native engine.

### Container images

`runs-on` labels map to images. Defaults use the community images built for local runners:

| Label | Image |
| --- | --- |
| `ubuntu-latest` | `ghcr.io/catthehacker/ubuntu:act-latest` |
| `ubuntu-24.04` / `22.04` / `20.04` | `ghcr.io/catthehacker/ubuntu:act-<version>` |

Override per run with `-P ubuntu-latest=node:20-bookworm`, or permanently in config. A job's
`container:` always takes precedence.

## Configuration

User config lives at `maverick config path` (e.g. `~/.config/maverick/config.toml` on Linux);
a project can add `.maverick.toml` at its root, which overrides it.

```toml
engine = "docker"
parallel = 4

[images]
ubuntu-latest = "ghcr.io/catthehacker/ubuntu:full-latest"

[engines.lima]
instance = "ci"

[engines.virtualbox]
ssh = "runner@127.0.0.1:2222"
identity = "~/.ssh/maverick"
```

## Secrets, variables and inputs

```sh
maverick run -s NPM_TOKEN=abc           # secrets.NPM_TOKEN
maverick run -s GITHUB_TOKEN            # value read from $GITHUB_TOKEN
maverick run --secret-file .secrets     # dotenv file
maverick run --var REGION=eu            # vars.REGION
maverick run -e workflow_dispatch -i greeting=Hey
maverick run --event-file payload.json  # custom github.event
```

Secret values (and anything passed to `::add-mask::`) are replaced with `***` in all output.

## Workflow support

Supported:

- `on` triggers, `env` and `defaults.run` at every level, `if:` with the full expression
  language (`contains`, `startsWith`, `format`, `toJSON`, `fromJSON`, `success()`,
  `failure()`, `always()`, `cancelled()`, ...)
- `needs` with outputs and results, parallel jobs, `strategy.matrix` with `include`/`exclude`,
  `fail-fast`
- `run` steps with `bash`, `sh`, `pwsh`, `powershell`, `cmd`, `python` and custom shells;
  `working-directory`, `continue-on-error`, `timeout-minutes`
- `$GITHUB_ENV`, `$GITHUB_OUTPUT`, `$GITHUB_PATH`, and the `::error::`, `::warning::`,
  `::group::`, `::add-mask::` workflow commands
- Actions: local (`./path`) and remote (`owner/repo@ref`, cached) composite and Node actions;
  `actions/checkout` is built in

Not yet supported: Docker-based actions, `services:`, reusable workflows (`jobs.<id>.uses`),
dynamic matrices, `actions/cache`, and artifact upload/download (these steps are skipped with a
warning rather than failing).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
maverick run CI -j lint --engine native   # maverick runs its own CI
```

`examples/demo` contains a workflow that exercises most features:
`maverick -C examples/demo`.

## License

MIT
