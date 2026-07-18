#!/usr/bin/env sh
# Fails if a GitHub App private key or a real-looking OAuth client secret is found anywhere in
# tracked source files, or (if a path is given) in a built binary. Milestone #1 (GitHub App
# migration) rule #1: neither may ever be committed to the repository, embedded in the compiled
# binary, or shipped in the frontend bundle, since the App is registered as a public OAuth client
# using PKCE and neither is ever required.
#
#   scripts/scan-for-secrets.sh [path/to/built/binary]
set -eu

fail=0
self="scripts/scan-for-secrets.sh"

echo "Scanning tracked source files for a private key..."
if git grep -nE -- '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----' -- ":!$self"; then
  echo "ERROR: found a private-key marker committed to the repository (see above)." >&2
  fail=1
fi

echo "Scanning tracked source files for a real-looking client secret value..."
if git grep -inE -- '(client[_-]?secret)["'"'"']?[[:space:]]*[:=][[:space:]]*["'"'"'][A-Za-z0-9_-]{20,}["'"'"']' -- ":!$self"; then
  echo "ERROR: found what looks like a real client secret value committed (see above)." >&2
  fail=1
fi

if [ "${1:-}" != "" ]; then
  binary="$1"
  echo "Scanning built binary ($binary) for a private key..."
  if grep -a -E -- '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----' "$binary"; then
    echo "ERROR: found a private-key marker embedded in the built binary." >&2
    fail=1
  fi
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "No secrets found."
