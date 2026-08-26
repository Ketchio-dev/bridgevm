#!/usr/bin/env bash
# One private-guest diagnostic pass over exactly 20 sealed AppX workloads.
set -euo pipefail
: "${TARGET:?}" "${VARS:?}" "${OUT:?}" "${VIOGPU3D_DIR:?}" "${CANDIDATES:?}"
REPO=$(cd "$(dirname "$0")/.." && pwd); cd "$REPO"
CASE="$OUT/case"; WORK="$OUT/work"; rm -rf "$WORK"; mkdir -p "$WORK" "$CASE/share"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"; chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
cp "$CANDIDATES" "$CASE/share/compatibility-candidates.tsv"
for asset in bvgpu-compatibility-observe.ps1 bvgpu-frametime-series.ps1; do cp "scripts/win-assets/$asset" "$CASE/share/$asset"; done
CTL="$CASE/agent.ctl"; INPUT="$CASE/input.ctl"; pid=''; : > "$CTL"; : > "$INPUT"
fail(){ echo "FAIL: $*" >&2; exit 1; }
wait_for(){
  local pattern="$1" count="$2" timeout="$3" deadline n log
  deadline=$((SECONDS+timeout))
  while (( SECONDS < deadline )); do
    log="${RUN:-$CASE}/run.log"; n=$(grep -acE "$pattern" "$log" 2>/dev/null || true)
    (( n >= count )) && return 0
    kill -0 "$pid" 2>/dev/null || return 1; sleep 1
  done
  return 1
}
send_ok(){
  local command="$1" timeout="${2:-300}" before line
  before=$(grep -ac '^BVAGENT END ' "$CASE/run.log" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$CTL"
  wait_for '^BVAGENT END ' $((before+1)) "$timeout" || return 1
  line=$(grep -aE '^BVAGENT CMD .* exit=' "$CASE/run.log" | tail -1)
  [[ "$line" == *' exit=0' ]]
}
# shellcheck source=scripts/pointer-reliability-vm.sh
source scripts/pointer-reliability-vm.sh
cleanup(){ local status=$?; pointer_vm_cleanup; rm -rf "$WORK"; return "$status"; }
trap cleanup EXIT INT TERM
pointer_vm_start_until_agent
for asset in compatibility-candidates.tsv bvgpu-compatibility-observe.ps1 bvgpu-frametime-series.ps1; do
  bytes=$(stat -f %z "$CASE/share/$asset")
  wait_for "^BVAGENT SHARE host->guest $asset bytes=$bytes " 1 600 || fail "share timeout: $asset"
done
command='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bvgpu-compatibility-observe.ps1 -Candidates C:\BridgeVMPtr\compatibility-candidates.tsv -OutDir C:\BridgeVMPtr -WarmupSeconds 5 -MeasureSeconds 30'
send_ok "$command" 2700 || fail 'guest compatibility observer failed'
for _ in $(seq 1 120); do
  [[ -s "$CASE/share/compatibility-observation.done" && -s "$CASE/share/observations.tsv" ]] && break
  sleep 1
done
[[ -s "$CASE/share/compatibility-observation.done" && -s "$CASE/share/observations.tsv" ]] || fail 'guest observation evidence did not return through the share'
validated=false
for _ in $(seq 1 120); do
  if python3 scripts/live-gates/validate-compatibility-observation.py --candidates "$CANDIDATES" \
    --results "$CASE/share/observations.tsv" --evidence "$CASE/share" --json-out "$OUT/observation-summary.json"; then validated=true; break; fi
  sleep 1
done
[[ "$validated" == true ]] || fail 'returned observation evidence did not verify'
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
kill -0 "$pid" 2>/dev/null && fail 'compatibility guest shutdown timeout'
wait "$pid" 2>/dev/null || fail 'compatibility guest launcher failed'
echo 'PASS: retained 20-AppX diagnostic observation'
