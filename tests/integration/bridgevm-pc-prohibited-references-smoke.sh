#!/usr/bin/env bash
# Fail-closed policy checks for the portable firmware reference scanner.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCAN="$ROOT/scripts/check-bridgevm-pc-prohibited-references.sh"
[[ -x "$SCAN" ]]
printf 'independent board\n' | "$SCAN" stream clean-fixture
! printf 'QEMU compatibility\n' | "$SCAN" stream bad-fixture >/dev/null 2>&1

work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
printf 'UEFI PCI\n' > "$work/source.txt"
"$SCAN" tree clean-tree "$work"
printf 'ArmVirt\n' > "$work/source.txt"
! "$SCAN" tree bad-tree "$work" >/dev/null 2>&1
! "$SCAN" tree missing-tree "$work/absent" >/dev/null 2>&1
! grep -q '\brg\b' "$SCAN"
echo "PASS: BridgeVM PC reference scanning is portable and fail closed"
