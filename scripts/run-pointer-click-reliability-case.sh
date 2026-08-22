#!/usr/bin/env bash
# Prepare and run one independent B4 click case. Required paths arrive via env.
set -euo pipefail
: "${RUN:?}" "${WORK:?}" "${TARGET:?}" "${VARS:?}" "${VIOGPU_DIR:?}"
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }
CASE="$RUN"; rm -rf "$WORK"; mkdir -p "$WORK" "$CASE/share"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"; chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1; do cp "scripts/win-assets/$asset" "$CASE/share/"; done
CTL="$CASE/agent.ctl"; INPUT="$CASE/input.ctl"; : > "$CTL"; : > "$INPUT"
wait_for() {
  local pattern="$1" count="$2" timeout="$3"
  local deadline=$((SECONDS + timeout)) observed previous=''
  while (( SECONDS < deadline )); do
    observed=$(grep -acE "$pattern" "$RUN/run.log" 2>/dev/null || true)
    [[ "$observed" == "$previous" ]] || { printf 'B4 wait pattern=%q observed=%q need=%q seconds=%q deadline=%q\n' "$pattern" "$observed" "$count" "$SECONDS" "$deadline" >&2; previous="$observed"; }
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    kill -0 "$pid" 2>/dev/null || return 1; sleep 1
  done
  return 1
}
send_ok() {
  local command="$1" before; before=$(grep -c '^BVAGENT END ' "$RUN/run.log" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$CTL"
  wait_for '^BVAGENT END ' $((before + 1)) 300 || return 1
  [[ $(grep -E '^BVAGENT CMD .* exit=' "$RUN/run.log" | tail -1) == *' exit=0' ]]
}
scanout_ready() { awk 'index($0,"\"name\":\"SET_SCANOUT\"") && index($0,"\"response_name\":\"OK_NODATA\"") && index($0,"\"rect_w\":1600,\"rect_h\":900") { found=1 } END { exit !found }' "$RUN/virtio-gpu.jsonl" 2>/dev/null; }
source scripts/pointer-reliability-vm.sh
trap pointer_vm_cleanup EXIT
pointer_vm_start_until_agent
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1; do
  wait_for "^BVAGENT SHARE host->guest $asset " 1 120 || fail "share timeout: $asset"
done
printf '%s\n' 'RESIZE 1600x900' >> "$INPUT"
wait_for '^live input accepted: resize=1600x900$' 1 30 || fail 'host resize not accepted'
send_ok 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bvgpu-apply-host-resolution.ps1 -Width 1600 -Height 900 -LaunchPointerTarget' || fail 'guest resize failed'
for _ in $(seq 1 120); do scanout_ready && break; sleep 1; done
scanout_ready || fail 'active 1600x900 scanout absent'
for _ in $(seq 1 120); do grep -q '^BVTARGET ready width=1600 height=900 ' "$CASE/share/bv-pointer-target-ready.log" 2>/dev/null && break; sleep 1; done
ready=$(tr -d '\r' < "$CASE/share/bv-pointer-target-ready.log"); [[ "$ready" =~ ^BVTARGET.ready.width=1600.height=900.screen_x=([-0-9]+).screen_y=([-0-9]+).center_x=([-0-9]+).center_y=([-0-9]+).virtual_x=([-0-9]+).virtual_y=([-0-9]+).virtual_w=([0-9]+).virtual_h=([0-9]+).hwnd=([1-9][0-9]*)$ ]] || fail 'target not ready'
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
