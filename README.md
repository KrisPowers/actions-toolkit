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

