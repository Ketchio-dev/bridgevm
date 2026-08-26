#!/usr/bin/env bash
# t8-pointer-reliability: B4's unchanged 20/20, p95 <=250 ms acceptance gate.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; SEALED_PACKAGE=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while (( $# )); do
  case "$1" in
    --out) OUT="$2"; shift 2 ;; --job-id) JOB_ID="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;; --sealed-package) SEALED_PACKAGE="$2"; shift 2 ;;
    *) echo "unknown pointer-tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$SEALED_PACKAGE" ]] || { echo 'pointer tier needs --out, --input-manifest, and --sealed-package' >&2; exit 2; }
mkdir -p "$OUT"
seal() { openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
value() { awk -F= -v key="$2" '$1==key{print substr($0,index($0,"=")+1)}' "$1"; }
PACKAGE_TOOL="$REPO/scripts/live-gates/b4-diagnostic-package.py"
manifest_hash=$(seal "$INPUT_MANIFEST"); package_hash="$($PACKAGE_TOOL verify --manifest "$INPUT_MANIFEST" --dir "$SEALED_PACKAGE")"
umd_hash=$(seal "$SEALED_PACKAGE/viogpu_d3d10.dll"); inf_hash=$(seal "$SEALED_PACKAGE/viogpu3d.inf")
"$REPO/scripts/check-hvf-windows-viogpu3d-package.sh" --require-render-candidate "$SEALED_PACKAGE" > "$OUT/package-check.log"
PREPARED=${PREPARED:-$HOME/BridgeVM/prepared/windows-1.0/d7a95823e889db5f4a24948be50653aaec92fb789adc8ff763c27c83be080b16-c61e2136c23b5e0a681f5d33810f617ae6ffc3ea7df0a950248c311767714265}
TARGET=${TARGET:-$PREPARED/disk.raw}; VARS=${VARS:-$PREPARED/vars.fd}
for input in "$TARGET" "$VARS"; do head -c1 "$input" >/dev/null 2>&1 || { echo "cannot read Windows media: $input" >&2; exit 1; }; done
source_image=$(seal "$TARGET"); source_vars=$(seal "$VARS")
[[ "$TARGET:$VARS" != "$PREPARED/disk.raw:$PREPARED/vars.fd" || "$(basename "$PREPARED")" == "$source_image-$source_vars" ]] || { echo 'prepared Windows media identity mismatch' >&2; exit 1; }
OUT="$OUT" SOURCE="$TARGET" SOURCE_VARS="$VARS" VIOGPU_DIR="$SEALED_PACKAGE" VIOGPU_MANIFEST="$INPUT_MANIFEST" \
  VIOGPU_PACKAGE_SHA256="$package_hash" VIOGPU_UMD_SHA256="$umd_hash" VIOGPU_INF_SHA256="$inf_hash" JOB_ID="$JOB_ID" \
  bash "$REPO/scripts/prepare-pointer-reliability-source.sh"
TARGET=$(value "$OUT/source.env" target); VARS=$(value "$OUT/source.env" vars)
IMAGE_HASH=$(seal "$TARGET"); VARS_HASH=$(seal "$VARS")
[[ "$(basename "$(dirname "$TARGET")")" == "$IMAGE_HASH-$VARS_HASH" && $(value "$OUT/source.env" viogpu_package_sha256) == "$package_hash" \
  && $(value "$OUT/source.env" driver_version) == 120.50.0.0 && $(value "$OUT/source.env" installed_umd_sha256) == "$umd_hash" ]] || { echo 'B4 prepared source identity mismatch' >&2; exit 1; }
status=0
N=20 OUT="$OUT/batch" TARGET="$TARGET" VARS="$VARS" VIOGPU3D_DIR="$SEALED_PACKAGE" \
  bash "$REPO/scripts/verify-pointer-click-reliability.sh" > "$OUT/gate.log" 2>&1 || status=$?
landed=$(sed -n 's/^landed \([0-9]*\/[0-9]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
p95=$(sed -n 's/.*p95_first_changed_ms=\([0-9a-z]*\).*/\1/p' "$OUT/batch/summary.txt" 2>/dev/null | tail -1)
outcome=completed; passed=(--pass); (( status == 0 )) || { outcome=failed; passed=(); }
python3 "$REPO/scripts/live-gates/write-pointer-reliability-receipt.py" --out "$OUT" --job-id "$JOB_ID" \
  --commit "$(git -C "$REPO" rev-parse HEAD)" --image "$IMAGE_HASH" --vars "$VARS_HASH" --manifest "$manifest_hash" \
  --package "$package_hash" --umd "$umd_hash" --landed "${landed:-unknown}" --p95 "${p95:-unknown}" --outcome "$outcome" ${passed[@]+"${passed[@]}"}
exit "$status"
