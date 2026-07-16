#!/usr/bin/env sh
# Updates Formula/actions-toolkit.rb to a new release version and its published sha256 checksums.
# Run this after the "Release" GitHub Actions workflow has finished publishing a tag's assets.
#
#   scripts/bump-formula.sh 0.2.0
set -eu

if [ "${1:-}" = "" ]; then
  echo "usage: scripts/bump-formula.sh <version>   (e.g. 0.2.0, no leading v)" >&2
  exit 1
fi

version="$1"
repo="KrisPowers/actions-toolkit"
formula="$(dirname "$0")/../Formula/actions-toolkit.rb"

fetch_sha() {
  asset="$1"
  url="https://github.com/${repo}/releases/download/v${version}/${asset}.tar.gz.sha256"
  curl -fsSL "$url" | awk '{print $1}'
}

echo "Fetching checksums for v${version}..."
macos_arm_sha="$(fetch_sha actions-toolkit-macos-aarch64)"
macos_x86_sha="$(fetch_sha actions-toolkit-macos-x86_64)"
linux_x86_sha="$(fetch_sha actions-toolkit-linux-x86_64)"

tmp="$(mktemp)"

# The sha256 line always immediately follows the url line for its platform, regardless of
# whether it currently holds the placeholder or a previous real checksum, so match on the
# preceding url line rather than on the sha256 value itself.
awk -v ver="$version" -v arm="$macos_arm_sha" -v x86="$macos_x86_sha" -v linux="$linux_x86_sha" '
  /version ".*"/ { sub(/version ".*"/, "version \"" ver "\""); print; next }
  /url .*macos-aarch64\.tar\.gz"/ { print; getline; print "      sha256 \"" arm "\""; next }
  /url .*macos-x86_64\.tar\.gz"/  { print; getline; print "      sha256 \"" x86 "\""; next }
  /url .*linux-x86_64\.tar\.gz"/  { print; getline; print "    sha256 \"" linux "\""; next }
  { print }
' "$formula" > "$tmp"

mv "$tmp" "$formula"

echo "Updated Formula/actions-toolkit.rb to v${version}."
echo "Review the diff, then commit it."
