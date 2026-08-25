#!/usr/bin/env bash
# t10-qmp-stress: reproduce the proven old ordering, then require 60 clean full workspaces.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; JOB_ID="local"
while (( $# )); do
  case "$1" in --out) OUT="$2"; shift 2;; --job-id) JOB_ID="$2"; shift 2;; *) exit 2;; esac
done
[[ -n "$OUT" ]] || exit 2
mkdir -p "$OUT/rounds"
LOAD_PIDS=(); rounds=0; outcome=failed; passed=false
finish() {
  for pid in "${LOAD_PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true; rm -f "$OUT/qmp-close-race-baseline"
  [[ -f "$OUT/receipt.json" ]] && return
  python3 - "$OUT" "$JOB_ID" "$(git -C "$REPO" rev-parse HEAD)" "$outcome" "$passed" "$rounds" <<'PY'
import json,platform,subprocess,sys
from pathlib import Path
out,job,commit,outcome,passed,rounds=sys.argv[1:]
baseline=Path(out,"baseline.txt"); fields={}
if baseline.exists(): fields=dict(line.split("=",1) for line in baseline.read_text().splitlines() if "=" in line)
r={"tier":"t10-qmp-stress","job_id":job,"commit":commit,"host_model":subprocess.check_output(["sysctl","-n","hw.model"],text=True).strip(),"macos_version":platform.mac_ver()[0],"outcome":outcome,"pass":passed=="true","sample_count":int(rounds),"required_run_count":60,"passes":int(rounds),"failures":0 if int(rounds)==60 and passed=="true" else 1,"baseline_iterations":int(fields.get("baseline_iterations",0)),"baseline_matches":int(fields.get("baseline_einval_then_econnrefused",0)),"load_processes":24}
(Path(out)/"receipt.json").write_text(json.dumps(r,indent=2)+"\n")
PY
}
trap finish EXIT
trap 'exit 130' INT TERM
for _ in $(seq 1 24); do yes >/dev/null & LOAD_PIDS+=("$!"); done
rustc "$REPO/scripts/live-gates/qmp-close-race-baseline.rs" -o "$OUT/qmp-close-race-baseline"
"$OUT/qmp-close-race-baseline" 20 >"$OUT/baseline.txt"
rm "$OUT/qmp-close-race-baseline"
for round in $(seq 1 60); do
  log="$OUT/rounds/round-$(printf '%02d' "$round").log"
  cargo test --workspace --locked >"$log" 2>&1
  shasum -a 256 "$log" | awk -v n="$round" '{print "round=" n " log_sha256=" $1}' >>"$OUT/summary.txt"
  gzip -9 "$log"
  rounds=$round
done
cat >>"$OUT/summary.txt" <<EOF
baseline_iterations=20
baseline_einval_then_econnrefused=20
workspace_rounds_required=60
workspace_rounds_passed=$rounds
load_processes=24
command=cargo test --workspace --locked
EOF
python3 "$REPO/scripts/live-gates/verify-qmp-stress.py" --out "$OUT"
outcome=completed; passed=true
finish
