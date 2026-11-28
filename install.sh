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
#   Installs the newest published General Preview; releases without the contract are refused.
set -euo pipefail

REPO="Ketchio-dev/bridgevm"
INSTALLER_SHA256="f848b8fb88c12176e2460b1bcc7bfd371e4a067211343d13de307782961acecd"
INSTALLER_URL="https://raw.githubusercontent.com/$REPO/main/scripts/install-bridgevm.sh"

fail() { echo "FAIL: $*" >&2; exit 1; }

# Unlike uname -m, this still reports Apple Silicon from a Rosetta shell.
if [[ "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null)" != 1 ]]; then
  fail "BridgeVM runs Windows 11 ARM on Apple Silicon Macs only"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# install.sh run from a repository checkout uses that checkout's copy; a
# curl|bash run has no script file and fetches the pinned one. Both paths end at
# the same hash check, so neither can drift quietly.
local_installer=""
if [[ -f "${BASH_SOURCE[0]:-}" ]]; then
  local_installer="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scripts/install-bridgevm.sh"
fi
if [[ -f "$local_installer" ]]; then
  cp "$local_installer" "$work/install-bridgevm.sh"
  mismatch="$local_installer does not match the installer this install.sh pins. Update the checkout so both files come from the same commit."
else
  curl -fsSL "$INSTALLER_URL" -o "$work/install-bridgevm.sh" ||
    fail "could not download the installer"
  mismatch="The downloaded installer didn't match the one this install.sh expects. GitHub can serve an older copy for a few minutes after an update; try again in a few minutes."
fi

actual=$(shasum -a 256 "$work/install-bridgevm.sh" | awk '{print $1}')
if [[ "$actual" != "$INSTALLER_SHA256" ]]; then
  fail "$mismatch Refusing to run an unverified installer (expected $INSTALLER_SHA256, got $actual)."
fi

exec bash "$work/install-bridgevm.sh" "$@"
