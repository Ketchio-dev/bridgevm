#!/usr/bin/env bash
# BridgeVM installer bootstrap.
#
# This thin script fetches the full fail-closed installer at a pinned SHA-256
# and refuses to run anything that does not match. The full installer verifies
# the release checksum, archive safety and code signature before it replaces
# anything, keeps the previous app as a rollback backup, and never touches VM
# data. scripts/check-install-bootstrap.sh keeps the pin in sync.
#
#   curl -fsSL https://raw.githubusercontent.com/Ketchio-dev/bridgevm/main/install.sh | bash
#   ... | bash -s -- --version v1.0.0 --dry-run   # options are forwarded
set -euo pipefail

REPO="Ketchio-dev/bridgevm"
INSTALLER_SHA256="3d972f3ce49d950e9e9ab3a419faa67a227ac854cc3a4ed18c65dcacd4e7e7c2"
INSTALLER_URL="https://raw.githubusercontent.com/$REPO/main/scripts/install-bridgevm.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }

if [[ "$(uname -s)/$(uname -m)" != "Darwin/arm64" ]]; then
  fail "BridgeVM runs Windows 11 ARM on Apple Silicon Macs only"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# A repository checkout runs its own copy; a curl|bash run fetches the pinned
# one. Both paths end at the same hash check, so neither can drift quietly.
local_installer="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd)/scripts/install-bridgevm.sh"
if [[ -f "$local_installer" ]]; then
  cp "$local_installer" "$work/install-bridgevm.sh"
else
  curl -fsSL "$INSTALLER_URL" -o "$work/install-bridgevm.sh" ||
    fail "could not download the installer"
fi

actual=$(shasum -a 256 "$work/install-bridgevm.sh" | awk '{print $1}')
if [[ "$actual" != "$INSTALLER_SHA256" ]]; then
  fail "installer hash mismatch (expected $INSTALLER_SHA256, got $actual); \
refusing to run an unverified installer. If a new release just shipped, \
re-fetch install.sh itself: the pin updates with the installer."
fi

exec bash "$work/install-bridgevm.sh" "$@"
