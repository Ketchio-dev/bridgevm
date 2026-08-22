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
[ -n "$OUT" ] || { echo "run-pointer-reliability-tier.sh needs --out" >&2; exit 2; }; mkdir -p "$OUT"
PREPARED=${PREPARED:-$HOME/BridgeVM/prepared/windows-1.0/d7a95823e889db5f4a24948be50653aaec92fb789adc8ff763c27c83be080b16-c61e2136c23b5e0a681f5d33810f617ae6ffc3ea7df0a950248c311767714265}
TARGET=${TARGET:-$PREPARED/disk.raw}; VARS=${VARS:-$PREPARED/vars.fd}
for input in "$TARGET" "$VARS"; do head -c1 "$input" >/dev/null 2>&1 || { echo "cannot read Windows media: $input" >&2; exit 1; }; done
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
source_image="$(seal "$TARGET")"; source_vars="$(seal "$VARS")"
[[ "$TARGET:$VARS" != "$PREPARED/disk.raw:$PREPARED/vars.fd" || "$(basename "$PREPARED")" == "$source_image-$source_vars" ]] || { echo 'prepared Windows media identity mismatch' >&2; exit 1; }
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
OUT="$OUT" SOURCE="$TARGET" SOURCE_VARS="$VARS" VIOGPU_DIR="$VIOGPU_DIR" JOB_ID="$JOB_ID" bash "$REPO/scripts/prepare-pointer-reliability-source.sh"
TARGET=$(awk -F= '$1=="target"{print substr($0,index($0,"=")+1)}' "$OUT/source.env"); VARS=$(awk -F= '$1=="vars"{print substr($0,index($0,"=")+1)}' "$OUT/source.env")
IMAGE_HASH="$(seal "$TARGET")"; VARS_HASH="$(seal "$VARS")"
[[ -n "$TARGET" && -n "$VARS" && "$(basename "$(dirname "$TARGET")")" == "$IMAGE_HASH-$VARS_HASH" ]] || { echo 'B4 prepared source identity mismatch' >&2; exit 1; }

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
  "image_sha256": "$IMAGE_HASH",
  "vars_sha256": "$VARS_HASH",
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
