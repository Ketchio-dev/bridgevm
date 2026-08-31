#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-nvme-block-tier.sh"
SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
CLI="$REPO/scripts/live-gates/bridgevm-live"
grep -q 'REQUIRED_LANES=20' "$RUN"
grep -q 'bridgevm-pc-standard-uefi-nvme-block-n20' "$RUN"
[[ "$(grep -Ec 'nvme_command=0x7|nvme_block_io=1 nvme_block_size=512' "$RUN")" -eq 2 ]]
grep -q 'codesign --sign - --entitlements' "$RUN"
grep -q 't12-bridgevm-pc-nvme-block)' "$SPECIAL"
queue="$(mktemp -d)"
trap 'rm -rf "$queue"' EXIT
job="$(BRIDGEVM_LIVE_ROOT="$queue" "$CLI" submit t12-bridgevm-pc-nvme-block)"
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" status "$job" | grep -q '^tier=t12-bridgevm-pc-nvme-block$'
echo "bridgevm pc nvme block live tier smoke: PASS"
