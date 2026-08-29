#!/usr/bin/env bash
# t8-pointer-reliability: the B4 20-clone click gate, sealed like every tier.
# Only this tier's receipt may close B4; a smaller N or a local run cannot.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    *) echo "unknown pointer-tier option $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT" ] || { echo "run-pointer-reliability-tier.sh needs --out" >&2; exit 2; }; mkdir -p "$OUT"
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
# shellcheck source=scripts/live-gates/pointer-input-manifest.sh
source "$REPO/scripts/live-gates/pointer-input-manifest.sh"
verify_pointer_manifest || { echo 'refused B4 unsealed or changed inputs' >&2; exit 1; }
TARGET="$(pointer_manifest_path image)"; VARS="$(pointer_manifest_path vars)"; VIOGPU_DIR="$(pointer_manifest_path viogpu_dir)"
for input in "$TARGET" "$VARS"; do head -c1 "$input" >/dev/null 2>&1 || { echo "cannot read Windows media: $input" >&2; exit 1; }; done
source_image="$(seal "$TARGET")"; source_vars="$(seal "$VARS")"
OUT="$OUT" SOURCE="$TARGET" SOURCE_VARS="$VARS" VIOGPU_DIR="$VIOGPU_DIR" JOB_ID="$JOB_ID" bash "$REPO/scripts/prepare-pointer-reliability-source.sh"
TARGET=$(awk -F= '$1=="target"{print substr($0,index($0,"=")+1)}' "$OUT/source.env"); VARS=$(awk -F= '$1=="vars"{print substr($0,index($0,"=")+1)}' "$OUT/source.env")
IMAGE_HASH="$(seal "$TARGET")"; VARS_HASH="$(seal "$VARS")"
[[ -n "$TARGET" && -n "$VARS" && "$(basename "$(dirname "$TARGET")")" == "$IMAGE_HASH-$VARS_HASH" ]] || { echo 'B4 prepared source identity mismatch' >&2; exit 1; }

status=0
N="${N:-20}" OUT="$OUT/batch" TARGET="$TARGET" VARS="$VARS" VIOGPU3D_DIR="$VIOGPU_DIR" \
  bash "$REPO/scripts/verify-pointer-click-reliability.sh" \
  > "$OUT/gate.log" 2>&1 || status=$?

landed=$(sed -n 's/^landed \([0-9]*\/[0-9]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
p95=$(sed -n 's/.*p95_first_changed_ms=\([0-9a-z]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
pointer_samples=$(sed -n 's/^landed [0-9]*\/\([0-9]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1); rendering_regressions=$(sed -n 's/.*rendering_package_regressions=\([0-9]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
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
  "driver_store_hash": "$(pointer_tree_hash "$VIOGPU_DIR")",
  "input_manifest_sha256": "$(seal "$INPUT_MANIFEST")",
  "host_model": "$(sysctl -n hw.model)",
  "macos_version": "$(sw_vers -productVersion)",
  "sample_count": ${N:-20}, "pointer_sample_count": ${pointer_samples:-0}, "rendering_package_regressions": ${rendering_regressions:-0},
  "landed": "${landed:-unknown}",
  "p95_first_changed_ms": "${p95:-unknown}",
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "outcome": "$outcome",
  "pass": $pass
}
EOF
exit "$status"
