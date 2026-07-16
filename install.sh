#!/usr/bin/env sh
# Installs the actions-toolkit CLI (backend + embedded UI in one binary).
#
#   curl -fsSL https://raw.githubusercontent.com/KrisPowers/actions-toolkit/main/install.sh | sh
#
# Env overrides:
#   ACTIONS_TOOLKIT_VERSION     release tag to install, e.g. v0.1.0 (default: latest)
#   ACTIONS_TOOLKIT_INSTALL_DIR directory to install the binary into (default: $HOME/.local/bin)
set -eu

REPO="KrisPowers/actions-toolkit"
BIN_NAME="actions-toolkit"
INSTALL_DIR="${ACTIONS_TOOLKIT_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${ACTIONS_TOOLKIT_VERSION:-latest}"

os="$(uname -s)"
case "$os" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *)
    echo "error: unsupported OS: $os (only Linux and macOS have prebuilt binaries)" >&2
    echo "You can still build from source: see README.md" >&2
    exit 1
    ;;
esac

arch="$(uname -m)"
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *)
    echo "error: unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

if [ "$platform" = "linux" ] && [ "$arch" = "aarch64" ]; then
  echo "error: no prebuilt linux/aarch64 binary yet; build from source instead (see README.md)" >&2
  exit 1
fi

asset="actions-toolkit-${platform}-${arch}"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}.tar.gz"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}.tar.gz"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Downloading actions-toolkit (${platform}-${arch}, ${VERSION})..."
if ! curl -fsSL "$url" -o "$tmp_dir/$asset.tar.gz"; then
  echo "error: download failed from $url" >&2
  echo "Check available versions at https://github.com/${REPO}/releases" >&2
  exit 1
fi

tar -xzf "$tmp_dir/$asset.tar.gz" -C "$tmp_dir"

mkdir -p "$INSTALL_DIR"
mv "$tmp_dir/$asset/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed actions-toolkit to $INSTALL_DIR/$BIN_NAME"

echo "Initializing data directory..."
"$INSTALL_DIR/$BIN_NAME" init

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "warning: $INSTALL_DIR is not on your PATH. Add this to your shell profile:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

echo ""
echo "Run '$BIN_NAME start' (or '$BIN_NAME listen') to launch actions-toolkit."
