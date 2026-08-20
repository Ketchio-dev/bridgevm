#!/usr/bin/env bash
# t8-pointer-reliability: the B4 20-clone click gate, sealed like every tier.
# Only this tier's receipt may close B4; a smaller N or a local run cannot.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown pointer-tier option $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { echo "run-pointer-reliability-tier.sh needs --out" >&2; exit 2; }
mkdir -p "$OUT"

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
for input in "$TARGET" "$VARS"; do
  head -c1 "$input" >/dev/null 2>&1 || {
    echo "cannot read required Windows media: $input" >&2
    exit 1
  }
done

seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }

status=0
N="${N:-20}" OUT="$OUT/batch" TARGET="$TARGET" VARS="$VARS" \
  bash "$REPO/scripts/verify-pointer-click-reliability.sh" \
  > "$OUT/gate.log" 2>&1 || status=$?

landed=$(sed -n 's/^landed \([0-9]*\/[0-9]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
p95=$(sed -n 's/.*p95_first_changed_ms=\([0-9a-z]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
outcome=completed; pass=true
[ "$status" -eq 0 ] || { outcome=failed; pass=false; }

# Fields must be on the redact-receipt allowlist or they are dropped.
cat > "$OUT/receipt.json" <<EOF
{
  "tier": "t8-pointer-reliability",
  "gate_id": "b4-pointer-click-reliability",
  "criterion": "B4",
  "job_id": "$JOB_ID",
  "commit": "$(git -C "$REPO" rev-parse HEAD)",
  "image_sha256": "$(seal "$TARGET")",
  "vars_sha256": "$(seal "$VARS")",
  "host_model": "$(sysctl -n hw.model)",
  "macos_version": "$(sw_vers -productVersion)",
  "sample_count": ${N:-20},
  "landed": "${landed:-unknown}",
  "p95_first_changed_ms": "${p95:-unknown}",
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "outcome": "$outcome",
  "pass": $pass
}
EOF
exit "$status"
