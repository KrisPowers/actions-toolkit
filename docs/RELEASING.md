# Releasing

Pushing a `v*` tag (e.g. `v0.2.0`) runs `.github/workflows/release.yml`, which compiles the
`atk` release binary for macOS (arm64 and x86_64), Linux (x86_64), and Windows (x86_64), and
attaches them (with `.sha256` checksums; Windows ships as `.zip`, the rest as `.tar.gz`) to a
GitHub Release. `install.sh`/`install.ps1` always download the `latest` release unless
`ATK_VERSION` is set.

After a release finishes, refresh the Homebrew formula's pinned version and checksums:

```bash
scripts/bump-formula.sh 0.2.0
```

Review the diff and commit `Formula/actions-toolkit.rb`.

## Install URL

`https://atk.krispowers.dev` is three Cloudflare Redirect Rules on the krispowers.dev zone (no
Worker, no hosting), checked in this order:

1. User agent contains `PowerShell`: 302 to `https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.ps1`
2. User agent starts with `curl/` or `Wget/`: 302 to `https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.sh`
3. Anything else: 302 to `https://github.com/KrisPowers/actions-toolkit`

The scripts are read from `main`, so changes go live on merge.
