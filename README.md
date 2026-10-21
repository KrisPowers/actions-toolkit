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

