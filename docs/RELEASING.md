# Releasing

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
