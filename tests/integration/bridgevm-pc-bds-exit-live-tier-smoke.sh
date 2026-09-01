#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-bds-exit-tier.sh"
SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
CLI="$REPO/scripts/live-gates/bridgevm-live"
grep -q 'REQUIRED_LANES=20' "$RUN"
grep -q 'bridgevm-pc-uefi-bds-exit-boot-services-n20' "$RUN"
grep -q 'stage=11 arch=0xfff filesystems=1' "$RUN"
grep -q 'exit_boot_services_attempts=\[1-3\]' "$RUN"
[[ "$(grep -Fc 'cp -c' "$RUN")" -eq 2 ]]
grep -q 'codesign --sign - --entitlements' "$RUN"
grep -q 't13-bridgevm-pc-bds-exit)' "$SPECIAL"
queue="$(mktemp -d)"
trap 'rm -rf "$queue"' EXIT
job="$(BRIDGEVM_LIVE_ROOT="$queue" "$CLI" submit t13-bridgevm-pc-bds-exit)"
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" status "$job" |
  grep -q '^tier=t13-bridgevm-pc-bds-exit$'
echo "bridgevm pc BDS/ExitBootServices live tier smoke: PASS"
