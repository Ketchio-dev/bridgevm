#!/usr/bin/env bash
# Install the sealed diagnostic UMD in one disposable clone and run one B4 scene.
set -euo pipefail
: "${RUN:?}" "${WORK:?}" "${TARGET:?}" "${VARS:?}" "${VIOGPU_DIR:?}" "${INPUT_MANIFEST:?}"
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }
CASE="$RUN"; parent="${CASE%/case}"; [[ -n "$parent" && "$parent" != / && "$CASE" == "$parent/case" && "$WORK" == "$parent/work" ]] || fail 'unsafe diagnostic work paths'; rm -rf "$WORK"; mkdir -p "$WORK" "$CASE/share"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"; chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
PACKAGE_TOOL=scripts/live-gates/b4-diagnostic-package.py
package_hash=$($PACKAGE_TOOL stage-share --manifest "$INPUT_MANIFEST" --dir "$VIOGPU_DIR" --share "$CASE/share")
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1 \
  b4-install-diagnostic-package.ps1 b4-dbwin-capture.ps1; do
  cp "scripts/win-assets/$asset" "$CASE/share/"
done
CTL="$CASE/agent.ctl"; INPUT="$CASE/input.ctl"; : > "$CTL"; : > "$INPUT"
dbwin_started=false; pid=''
wait_for() {
  local pattern="$1" count="$2" timeout="$3"
  local deadline=$((SECONDS + timeout)) observed previous=''
  while (( SECONDS < deadline )); do
    observed=$(grep -acE "$pattern" "$RUN/run.log" 2>/dev/null || true)
    [[ "$observed" == "$previous" ]] || { printf 'B4 diagnostic wait pattern=%q observed=%q need=%q seconds=%q deadline=%q\n' "$pattern" "$observed" "$count" "$SECONDS" "$deadline" >&2; previous="$observed"; }
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    kill -0 "$pid" 2>/dev/null || return 1; sleep 1
  done
  return 1
}
send_ok() {
  local command="$1" before; before=$(grep -ac '^BVAGENT END ' "$RUN/run.log" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$CTL"
  wait_for '^BVAGENT END ' $((before + 1)) 300 || return 1
  [[ $(grep -aE '^BVAGENT CMD .* exit=' "$RUN/run.log" | tail -1) == *' exit=0' ]]
}
wait_shared_file() {
  local path="$1" pattern="$2" timeout="$3"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    tr -d '\r' < "$path" 2>/dev/null | grep -q "$pattern" && return 0
    kill -0 "$pid" 2>/dev/null || return 1; sleep 1
  done
  return 1
}
scanout_ready() { awk 'index($0,"\"name\":\"SET_SCANOUT\"") && index($0,"\"response_name\":\"OK_NODATA\"") && index($0,"\"rect_w\":1600,\"rect_h\":900") { found=1 } END { exit !found }' "$RUN/virtio-gpu.jsonl" 2>/dev/null; }
relink_generation() {
  for path in run.log launcher.out virtio-gpu.jsonl active-scanout.fb active-scanout.fb.iosurface visible; do
    ln -sfn "generation-$generation/$path" "$CASE/$path"
  done
  printf 'stable_generation=%s\n' "$generation" > "$CASE/reset.env"
}
# shellcheck disable=SC1091
source scripts/pointer-reliability-vm.sh
cleanup() {
  [[ "$dbwin_started" == false ]] || : > "$CASE/share/b4-dbwin-stop.request"
  pointer_vm_cleanup
}
trap cleanup EXIT
pointer_vm_start_until_agent
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1 \
  b4-install-diagnostic-package.ps1 b4-dbwin-capture.ps1 b4-package-manifest.tsv; do
  wait_for "^BVAGENT SHARE host->guest $asset " 1 1200 || fail "share timeout: $asset"
done
while IFS=$'\t' read -r kind _ _ chunk _ _; do
  [[ "$kind" == chunk ]] || continue
  wait_for "^BVAGENT SHARE host->guest $chunk " 1 1200 || fail "share timeout: $chunk"
done < "$CASE/share/b4-package-manifest.tsv"
install_command="powershell -NoProfile -Command \"Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMPtr\\b4-install-diagnostic-package.ps1' } | Out-Null; Write-Output B4INSTALL_LAUNCHED\""
send_ok "$install_command" || fail 'diagnostic package installer launch failed'
wait_shared_file "$CASE/share/b4-install-result.log" '^install_ready=true$' 600 || fail 'diagnostic package install failed'
grep -q "^package_sha256=$package_hash" "$CASE/share/b4-install-result.log" || fail 'guest install package hash mismatch'
send_ok 'powershell -NoProfile -Command "Get-ChildItem -LiteralPath C:\BridgeVMPtr -Filter b4pkg-*.bin -File | Remove-Item -Force"' || fail 'diagnostic package staging cleanup failed'
for _ in $(seq 1 180); do compgen -G "$CASE/share/b4pkg-*.bin" >/dev/null || break; sleep 1; done
if compgen -G "$CASE/share/b4pkg-*.bin" >/dev/null; then fail 'diagnostic package staging deletion did not propagate'; fi
send_ok 'shutdown /r /t 3' || fail 'diagnostic reboot request failed'
for _ in $(seq 1 180); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
kill -0 "$pid" 2>/dev/null && fail 'diagnostic reboot boundary timeout'
set +e; wait "$pid"; reset_status=$?; set -e
[[ "$reset_status" -eq 42 ]] || fail "diagnostic reboot helper exited $reset_status"
generation=$((generation + 1)); : > "$CTL"; : > "$INPUT"
pointer_vm_launch
wait_for '^BVAGENT SERVICE alive' 1 1200 || fail 'agent absent after diagnostic reboot'
relink_generation
verify_command="powershell -NoProfile -Command \"Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMPtr\\b4-install-diagnostic-package.ps1 -VerifyOnly' } | Out-Null; Write-Output B4VERIFY_LAUNCHED\""
send_ok "$verify_command" || fail 'diagnostic identity verifier launch failed'
wait_shared_file "$CASE/share/b4-verify-result.log" '^verified=true$' 600 || fail 'installed diagnostic identity verification failed'
grep -q "^package_sha256=$package_hash" "$CASE/share/b4-verify-result.log" || fail 'installed package tree hash mismatch'

dbwin_command="powershell -NoProfile -Command \"Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMPtr\\b4-dbwin-capture.ps1 -DurationMs 900000' } | Out-Null; Write-Output B4DBWIN_LAUNCHED\""
send_ok "$dbwin_command" || fail 'DBWIN capture launch failed'; dbwin_started=true
wait_shared_file "$CASE/share/b4-dbwin-ready.log" '^B4DBWIN ready=true$' 120 || fail 'DBWIN capture not ready'
wait_shared_file "$CASE/share/b4-dbwin.log" '^\[dbwin\] capture_ready$' 30 || fail 'DBWIN log not visible at window start'
dbwin_skip=$(wc -l < "$CASE/share/b4-dbwin.log"); trace_skip=$(wc -l < "$RUN/virtio-gpu.jsonl")
[[ "$dbwin_skip" =~ ^[0-9]+$ && "$trace_skip" =~ ^[0-9]+$ ]] || fail 'diagnostic window offsets invalid'
printf 'skip_dbwin_lines=%s\nskip_trace_lines=%s\n' "$dbwin_skip" "$trace_skip" > "$CASE/analysis-window.env"

printf '%s\n' 'RESIZE 1600x900' >> "$INPUT"
wait_for '^live input accepted: resize=1600x900$' 1 30 || fail 'host resize not accepted'
send_ok 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bvgpu-apply-host-resolution.ps1 -Width 1600 -Height 900 -LaunchPointerTarget' || fail 'guest resize failed'
for _ in $(seq 1 120); do scanout_ready && break; sleep 1; done
scanout_ready || fail 'active 1600x900 scanout absent'
for _ in $(seq 1 120); do grep -q '^BVTARGET ready width=1600 height=900 ' "$CASE/share/bv-pointer-target-ready.log" 2>/dev/null && break; sleep 1; done
ready=$(tr -d '\r' < "$CASE/share/bv-pointer-target-ready.log" 2>/dev/null || true)
[[ "$ready" =~ ^BVTARGET.ready.width=1600.height=900.screen_x=([-0-9]+).screen_y=([-0-9]+).center_x=([-0-9]+).center_y=([-0-9]+).virtual_x=([-0-9]+).virtual_y=([-0-9]+).virtual_w=([0-9]+).virtual_h=([0-9]+).hwnd=([1-9][0-9]*)$ ]] || fail 'target not ready'
sx=${BASH_REMATCH[1]}; sy=${BASH_REMATCH[2]}; cx=${BASH_REMATCH[3]}; cy=${BASH_REMATCH[4]}
hid_x=$(( (cx - sx) * 32767 / 1599 )); hid_y=$(( (cy - sy) * 32767 / 899 ))
printf 'POINTER move:%sx%s\n' "$hid_x" "$hid_y" >> "$INPUT"
for _ in $(seq 1 120); do [[ -s "$RUN/active-scanout.fb.iosurface" ]] && break; sleep 1; done
[[ -s "$RUN/active-scanout.fb.iosurface" ]] || fail 'active CGL IOSurface absent'; sleep 2
send_ok 'powershell -NoProfile -Command "Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = '\''cmd /c powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bv-pointer-capture.ps1 -DurationMs 20000 -ReadyPath C:\BridgeVMPtr\bvptr-ready.log -StopAfterClick > C:\BridgeVMPtr\bvptr.log 2>&1'\'' } | Out-Null; Write-Output BVPTR_LAUNCHED"' || fail 'probe launch failed'
for _ in $(seq 1 120); do grep -q "^BVPTR_READY cursor_x=$cx cursor_y=$cy" "$CASE/share/bvptr-ready.log" 2>/dev/null && break; sleep 1; done
grep -q "^BVPTR_READY cursor_x=$cx cursor_y=$cy" "$CASE/share/bvptr-ready.log" || fail 'probe not ready at target'
python3 scripts/watch-pointer-visible-reaction.py --iosurface "$RUN/active-scanout.fb.iosurface" --input-control "$INPUT" --out "$RUN/visible" --hid-x "$hid_x" --hid-y "$hid_y" || true
for _ in $(seq 1 60); do grep -q 'BVPTR summary' "$CASE/share/bvptr.log" 2>/dev/null && break; sleep 1; done
grep -q 'BVPTR summary' "$CASE/share/bvptr.log" || fail 'probe summary absent'
send_ok 'powershell -NoProfile -Command "New-Item -ItemType File -Path C:\BridgeVMPtr\b4-dbwin-stop.request -Force | Out-Null"' || fail 'DBWIN stop request failed'
wait_shared_file "$CASE/share/b4-dbwin-complete.log" '^B4DBWIN complete=true$' 120 || fail 'DBWIN capture did not complete'
dbwin_started=false
wait_shared_file "$CASE/share/b4-dbwin.log" '^\[dbwin\] capture_ready$' 30 || fail 'retained DBWIN log absent'
send_ok 'shutdown /s /t 3' || fail 'diagnostic VM shutdown request failed'
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
kill -0 "$pid" 2>/dev/null && fail 'diagnostic VM shutdown timeout'
wait "$pid" || fail 'diagnostic VM launcher failed during shutdown'; pid=''
python3 scripts/live-gates/analyze-b4-umd-diagnostic.py \
  --dbwin "$CASE/share/b4-dbwin.log" --trace "$RUN/virtio-gpu.jsonl" \
  --skip-dbwin-lines "$dbwin_skip" --skip-trace-lines "$trace_skip" \
  --out "$CASE/correlation.json" > "$CASE/correlation.out"
outcome=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["outcome"])' "$CASE/correlation.json")
correlated=$(python3 -c 'import json,sys; print(str(json.load(open(sys.argv[1]))["correlated"]).lower())' "$CASE/correlation.json")
submit_count=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["dbwin_submit_count"])' "$CASE/correlation.json")
max_allocations=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["max_submit_allocations"])' "$CASE/correlation.json")
visual=unmeasured
if [[ -f "$RUN/visible/visible.env" ]]; then
  first=$(awk -F= '$1=="first_changed_ms"{print $2}' "$RUN/visible/visible.env")
  [[ "$first" == none || -z "$first" ]] && visual=no-visible-change || visual=changed
elif [[ -f "$RUN/visible/baseline.env" ]]; then
  peak=$(awk -F= '$1=="peak_white_px"{print $2}' "$RUN/visible/baseline.env")
  [[ "$peak" == 0 ]] && visual=stable-black || visual=baseline-unsettled
fi
cat > "$CASE/summary.txt" <<EOF
criterion=b4-diagnostic-only
package_sha256=$package_hash
stable_generation=$generation
host_trace=$(basename "$RUN/virtio-gpu.jsonl")
dbwin_log=b4-dbwin.log
skip_dbwin_lines=$dbwin_skip
skip_trace_lines=$trace_skip
correlation_outcome=$outcome
correlated=$correlated
submit_event_count=$submit_count
max_submit_allocations=$max_allocations
visual_outcome=$visual
b4_acceptance=unmeasured
EOF
