#!/usr/bin/env bash
# Static and queue-policy checks for the fixed-N NVMe BAR tier.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-nvme-bar-tier.sh"
SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
CLI="$REPO/scripts/live-gates/bridgevm-live"
[[ -x "$RUN" && -x "$SPECIAL" && -x "$CLI" ]]
grep -q 'readonly REQUIRED_LANES=20' "$RUN"
grep -q 'lane_dir="$OUT/lanes/$lane_name"' "$RUN"
grep -q 'bridgevm-pc-standard-uefi-nvme-bar-n20' "$RUN"
grep -q 'nvme_bar_reads=2 .*nvme_cap=0x20020103ff .*nvme_version=0x10400' "$RUN"
grep -q 't11-bridgevm-pc-nvme-bar) helper=run-bridgevm-pc-nvme-bar-tier.sh' "$SPECIAL"
! grep -Eq 'N:-|sleep [0-9]{3}|sudo|actions-runner' "$RUN" "$SPECIAL"
queue="$(mktemp -d)"; trap 'rm -rf "$queue"' EXIT
job="$(BRIDGEVM_LIVE_ROOT="$queue" "$CLI" submit t11-bridgevm-pc-nvme-bar)"
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" status "$job" | grep -q '^tier=t11-bridgevm-pc-nvme-bar$'
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" cancel "$job" | grep -q "canceled $job"
echo "PASS: BridgeVM PC NVMe BAR tier is fixed at 20 isolated lanes"
