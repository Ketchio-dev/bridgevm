#!/usr/bin/env bash
# Dispatch sealed or development-specialized Studio tiers.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="${1:?run-special-tier.sh needs a tier}"
OUT="${2:?run-special-tier.sh needs an output directory}"
JOB_ID="${3:?run-special-tier.sh needs a job id}"
INPUT_MANIFEST="${4:-}"
args=(--out "$OUT" --job-id "$JOB_ID")
case "$TIER" in
  t8-pointer-reliability)
    helper=run-pointer-reliability-tier.sh
    args+=(--input-manifest "$INPUT_MANIFEST") ;;
  t9-bridgevm-pc-pci) helper=run-bridgevm-pc-pci-tier.sh ;;
  t10-qmp-stress) helper=run-qmp-stress-tier.sh ;;
  t11-bridgevm-pc-nvme-bar) helper=run-bridgevm-pc-nvme-bar-tier.sh ;;
  *) echo "unknown special tier $TIER" >&2; exit 2 ;;
esac
"$REPO/scripts/live-gates/$helper" "${args[@]}"
