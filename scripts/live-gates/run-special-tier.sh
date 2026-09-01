#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="${1:?run-special-tier.sh needs a tier}"; OUT="${2:?run-special-tier.sh needs an output directory}"
JOB_ID="${3:?run-special-tier.sh needs a job id}"; args=(--out "$OUT" --job-id "$JOB_ID")
case "$TIER" in
  t8-pointer-reliability)
    helper=run-pointer-reliability-tier.sh
    args+=(--input-manifest "${4:-}") ;;
  t9-bridgevm-pc-pci) helper=run-bridgevm-pc-pci-tier.sh ;;
  t10-qmp-stress) helper=run-qmp-stress-tier.sh ;;
  t11-bridgevm-pc-nvme-bar) helper=run-bridgevm-pc-nvme-bar-tier.sh ;;
  t12-bridgevm-pc-nvme-block) helper=run-bridgevm-pc-nvme-block-tier.sh ;;
  t13-bridgevm-pc-bds-exit) helper=run-bridgevm-pc-bds-exit-tier.sh ;;
  t14-bridgevm-pc-windows-start) helper=run-bridgevm-pc-windows-start-tier.sh; args+=(--input-manifest "${4:-}") ;;
  t15-hvf-boot-performance|t16-hvf-nvme-performance) helper=run-hvf-boot-performance-tier.sh; [[ "$TIER" == t15-* ]] || helper=run-hvf-nvme-performance-tier.sh; args+=(--input-manifest "${4:-}" --sealed-binary "${5:-}") ;;
  *) echo "unknown special tier $TIER" >&2; exit 2 ;;
esac
"$REPO/scripts/live-gates/$helper" "${args[@]}"
