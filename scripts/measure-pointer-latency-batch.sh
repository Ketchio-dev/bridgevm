#!/usr/bin/env bash
# B4 needs a denominator: one run that reacts and one that does not is not a
# rate. This boots the guest N times through measure-pointer-latency.sh and
# reports reacted k/N plus each run's first visible change.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1

N=${N:-10}
SUMMARY=${SUMMARY:-$HOME/BridgeVM/runs/pointer-batch-$(date +%Y%m%d-%H%M%S).txt}
mkdir -p "$(dirname "$SUMMARY")"

reacted=0
{
  echo "pointer latency batch: n=$N started $(date -u +%FT%TZ)"
  for i in $(seq 1 "$N"); do
    out=$(OUT="$HOME/BridgeVM/runs/pointer-batch-run$i" bash scripts/measure-pointer-latency.sh 2>&1 |
      grep -E 'first_changed_ms=|B4 pointer latency' || true)
    first=$(sed -n 's/.*first_changed_ms=\([0-9a-z]*\).*/\1/p' <<<"$out" | head -1)
    if [[ -n "$first" && "$first" != "none" ]]; then
      reacted=$((reacted + 1))
      echo "run $i: reacted first_changed_ms=$first"
    else
      echo "run $i: no reaction"
    fi
  done
  echo "reacted $reacted/$N"
} | tee "$SUMMARY"
echo "summary: $SUMMARY"
