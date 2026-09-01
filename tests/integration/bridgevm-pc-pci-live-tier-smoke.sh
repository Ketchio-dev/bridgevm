#!/usr/bin/env bash
# Static and queue-policy checks for the fixed-N experimental-board PCI tier.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
RUN="$REPO/scripts/live-gates/run-bridgevm-pc-pci-tier.sh"
SPECIAL="$REPO/scripts/live-gates/run-special-tier.sh"
CLI="$REPO/scripts/live-gates/bridgevm-live"
[[ -x "$RUN" && -x "$SPECIAL" && -x "$CLI" ]]
grep -q 'readonly REQUIRED_LANES=20' "$RUN"
grep -q 'lane_dir="$OUT/lanes/$lane_name"' "$RUN"
grep -q 'bridgevm-pc-variable-process-persistence.sh' "$RUN"
grep -q '"sample_count": $attempted' "$RUN"
grep -q '"required_run_count": $REQUIRED_LANES' "$RUN"
grep -q 't9-bridgevm-pc-pci) helper=run-bridgevm-pc-pci-tier.sh' "$SPECIAL"
! grep -Eq 'N:-|sleep [0-9]{3}|sudo|actions-runner' "$RUN" "$SPECIAL"

queue="$(mktemp -d)"; trap 'rm -rf "$queue"' EXIT
job="$(BRIDGEVM_LIVE_ROOT="$queue" "$CLI" submit t9-bridgevm-pc-pci)"
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" status "$job" | grep -q '^tier=t9-bridgevm-pc-pci$'
BRIDGEVM_LIVE_ROOT="$queue" "$CLI" cancel "$job" | grep -q "canceled $job"
echo "PASS: BridgeVM PC standard UEFI PCI tier is fixed at 20 isolated lanes"
