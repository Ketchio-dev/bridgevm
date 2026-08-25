#!/usr/bin/env bash
# t11-glyph-scene-pilot: retain one exact active-CGL scene, never a glyph pass.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"; OUT=""; JOB_ID=local
while (( $# )); do case "$1" in --out) OUT="$2";shift 2;;--job-id) JOB_ID="$2";shift 2;;*)exit 2;;esac; done
[[ -n "$OUT" ]]||exit 2; mkdir -p "$OUT"; cleanup(){ rm -rf "$OUT/work"; }; trap cleanup EXIT INT TERM
PREPARED=${PREPARED:-$HOME/BridgeVM/prepared/windows-1.0/d7a95823e889db5f4a24948be50653aaec92fb789adc8ff763c27c83be080b16-c61e2136c23b5e0a681f5d33810f617ae6ffc3ea7df0a950248c311767714265}
TARGET="$PREPARED/disk.raw"; VARS="$PREPARED/vars.fd"; VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
seal(){ openssl dgst -sha256 -r "$1"|cut -d' ' -f1; }; image=$(seal "$TARGET"); vars=$(seal "$VARS")
[[ "$(basename "$PREPARED")" == "$image-$vars" ]]||{ echo 'prepared identity mismatch' >&2; exit 1; }
status=0; RUN="$OUT/scene" WORK="$OUT/work" TARGET="$TARGET" VARS="$VARS" VIOGPU_DIR="$VIOGPU_DIR" bash "$REPO/scripts/run-glyph-scene-pilot-case.sh" >"$OUT/gate.log" 2>&1||status=$?
python3 - "$OUT" "$JOB_ID" "$(git -C "$REPO" rev-parse HEAD)" "$image" "$vars" "$status" <<'PY'
import json,platform,subprocess,sys
from pathlib import Path
out,job,commit,image,vars,status=Path(sys.argv[1]),*sys.argv[2:]; passed=status=="0"
r={"tier":"t11-glyph-scene-pilot","gate_id":"glyph-scene-channel","criterion":"glyph-diagnostic-only","job_id":job,"commit":commit,"image_sha256":image,"vars_sha256":vars,"host_model":subprocess.check_output(["sysctl","-n","hw.model"],text=True).strip(),"macos_version":platform.mac_ver()[0],"sample_count":1,"active_scanout_capture":(out/"scene/glyph-scene/presented.ppm").is_file(),"outcome":"completed" if passed else "failed","pass":passed,"known_confounders":["Diagnostic scene identity only; glyph correctness remains unmeasured."]}
(out/"receipt.json").write_text(json.dumps(r,indent=2)+"\n")
PY
exit "$status"
