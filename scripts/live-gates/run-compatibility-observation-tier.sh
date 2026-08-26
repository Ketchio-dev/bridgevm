#!/usr/bin/env bash
# t13: diagnostic-only observation used to seal a later compatibility matrix.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; SEALED_INPUTS=""; SEALED_PACKAGE=""; JOB_ID=local
while (( $# )); do
  case "$1" in
    --out) OUT="$2"; shift 2 ;; --job-id) JOB_ID="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;; --sealed-inputs) SEALED_INPUTS="$2"; shift 2 ;;
    --sealed-package) SEALED_PACKAGE="$2"; shift 2 ;; *) exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$SEALED_INPUTS" && -n "$SEALED_PACKAGE" ]] || exit 2
mkdir -p "$OUT"
seal(){ openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'; }
value(){ awk -F= -v key="$2" '$1==key{print substr($0,index($0,"=")+1)}' "$1"; }
path(){ awk -F '\t' -v key="$2" '$1==key{print $2}' "$1"; }
INPUT_TOOL="$REPO/scripts/live-gates/compatibility-observation-input.py"
PACKAGE_TOOL="$REPO/scripts/live-gates/b4-diagnostic-package.py"
candidate_hash="$($INPUT_TOOL verify --manifest "$INPUT_MANIFEST" --dir "$SEALED_INPUTS")"
candidates="$SEALED_INPUTS/sealed-candidates.tsv"; b4_manifest="$SEALED_INPUTS/b4-input-manifest.tsv"
package_hash="$($PACKAGE_TOOL verify --manifest "$b4_manifest" --dir "$SEALED_PACKAGE")"
umd_hash=$(seal "$SEALED_PACKAGE/viogpu_d3d10.dll"); inf_hash=$(seal "$SEALED_PACKAGE/viogpu3d.inf")
"$REPO/scripts/check-hvf-windows-viogpu3d-package.sh" --require-render-candidate "$SEALED_PACKAGE" > "$OUT/package-check.log"
SOURCE=$(path "$INPUT_MANIFEST" image); SOURCE_VARS=$(path "$INPUT_MANIFEST" vars)
OUT="$OUT" SOURCE="$SOURCE" SOURCE_VARS="$SOURCE_VARS" VIOGPU_DIR="$SEALED_PACKAGE" \
  VIOGPU_MANIFEST="$b4_manifest" VIOGPU_PACKAGE_SHA256="$package_hash" \
  VIOGPU_UMD_SHA256="$umd_hash" VIOGPU_INF_SHA256="$inf_hash" JOB_ID="$JOB_ID" \
  bash "$REPO/scripts/prepare-pointer-reliability-source.sh"
TARGET=$(value "$OUT/source.env" target); VARS=$(value "$OUT/source.env" vars)
image_hash=$(seal "$TARGET"); vars_hash=$(seal "$VARS"); status=0
TARGET="$TARGET" VARS="$VARS" OUT="$OUT/observation" VIOGPU3D_DIR="$SEALED_PACKAGE" CANDIDATES="$candidates" \
  bash "$REPO/scripts/run-windows-compatibility-observation.sh" > "$OUT/gate.log" 2>&1 || status=$?
python3 - "$OUT" "$JOB_ID" "$(git -C "$REPO" rev-parse HEAD)" "$image_hash" "$vars_hash" \
  "$(seal "$INPUT_MANIFEST")" "$candidate_hash" "$package_hash" "$status" <<'PY'
import json,platform,subprocess,sys
from pathlib import Path
out=Path(sys.argv[1]); job,commit,image,vars,manifest,candidates,package,status=sys.argv[2:]
summary_path=out/"observation/observation-summary.json"
summary=json.loads(summary_path.read_text()) if summary_path.is_file() else {}
passed=status=="0" and summary.get("rows_observed")==20 and summary.get("identities_verified")==20
r={"tier":"t13-compatibility-observation","gate_id":"windows-20-workload-observation","criterion":"compatibility-diagnostic-only","job_id":job,"commit":commit,"image_sha256":image,"vars_sha256":vars,"input_manifest_sha256":manifest,"compatibility_candidates_sha256":candidates,"sealed_package_sha256":package,"host_model":subprocess.check_output(["sysctl","-n","hw.model"],text=True).strip(),"macos_version":platform.mac_ver()[0],"sample_count":int(summary.get("rows_observed",0)),"required_run_count":20,"identities_verified":int(summary.get("identities_verified",0)),"apps_launched":int(summary.get("apps_launched",0)),"apps_visible":int(summary.get("apps_visible",0)),"clean_shutdowns":int(summary.get("clean_shutdowns",0)),"series_rows":int(summary.get("series_rows",0)),"vulkan_module_rows":int(summary.get("vulkan_module_rows",0)),"d3d11_module_rows":int(summary.get("d3d11_module_rows",0)),"d3d12_module_rows":int(summary.get("d3d12_module_rows",0)),"opengl_module_rows":int(summary.get("opengl_module_rows",0)),"compatibility_observation_sha256":summary.get("observation_sha256","absent"),"outcome":"completed" if passed else "failed","pass":passed,"known_confounders":["Diagnostic API/process observation only; it cannot satisfy the 20-workload compatibility contract."]}
(out/"receipt.json").write_text(json.dumps(r,indent=2)+"\n")
PY
exit "$status"
