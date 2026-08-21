#!/usr/bin/env bash
# The bootstrap installer pins the full installer by SHA-256. A stale pin is
# fail-closed for users (the bootstrap refuses to run), but it would still be a
# broken install path shipped from main, so drift fails the project check.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

pinned=$(sed -n 's/^INSTALLER_SHA256="\([0-9a-f]\{64\}\)"$/\1/p' install.sh)
[[ -n "$pinned" ]] || {
  echo "install bootstrap: FAIL (install.sh has no INSTALLER_SHA256 pin)" >&2
  exit 1
}
actual=$(shasum -a 256 scripts/install-bridgevm.sh | awk '{print $1}')
if [[ "$pinned" != "$actual" ]]; then
  echo "install bootstrap: FAIL (install.sh pins $pinned but" \
    "scripts/install-bridgevm.sh hashes to $actual; update the pin)" >&2
  exit 1
fi
echo "install bootstrap: PASS (pin $pinned)"
