#!/usr/bin/env bash
# Prepare and run one independent B4 click case. Required paths arrive via env.
set -euo pipefail
: "${RUN:?}" "${WORK:?}" "${TARGET:?}" "${VARS:?}" "${VIOGPU_DIR:?}" "${DELAYS:?}"
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }
rm -rf "$WORK"; mkdir -p "$WORK" "$RUN/share"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"; chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1; do cp "scripts/win-assets/$asset" "$RUN/share/"; done
CTL="$RUN/agent.ctl"; INPUT="$RUN/input.ctl"; : > "$CTL"; : > "$INPUT"
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
cleanup() {
  if kill -0 "$pid" 2>/dev/null; then printf '%s\n' 'shutdown /s /t 3' >> "$CTL"; fi
  for _ in $(seq 1 60); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
}
BRIDGEVM_TRACE_DCI5_EMISSION=1 BRIDGEVM_XHCI_REPORT_INTERVAL_MS=200 \
scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" --evidence-dir "$RUN" \
  --watchdog-ms 720000 --ram-mib 6144 --smp-cpus 4 --release --enable-xhci \
  --input-control "$INPUT" --pointer-input-actions 'click:16384x16384' \
  --pointer-input-fire-delay-ms 600000 --pointer-input-ramfb-delay-ms "$DELAYS" \
  --display-export-ppm "$RUN/active-scanout.ppm" --display-export-fb "$RUN/active-scanout.fb" \
  --display-export-ms 100 --agent-service-control "$CTL" \
  --agent-share-host "$RUN/share" --agent-share-guest 'C:\BridgeVMPtr' --agent-share-ms 500 \
  --virtio-gpu-3d --gpu-trace "$RUN/virtio-gpu.jsonl" --gpu-trace-protocol venus \
  --viogpu3d-dir "$VIOGPU_DIR" > "$RUN/launcher.out" 2>&1 &
pid=$!; trap cleanup EXIT
wait_for '^BVAGENT SERVICE alive' 1 1200 || fail 'agent service timeout'
for asset in bv-pointer-capture.ps1 bv-pointer-target.ps1 bvgpu-apply-host-resolution.ps1; do
  wait_for "^BVAGENT SHARE host->guest $asset " 1 120 || fail "share timeout: $asset"
done
printf '%s\n' 'RESIZE 1600x900' >> "$INPUT"
wait_for '^live input accepted: resize=1600x900$' 1 30 || fail 'host resize not accepted'
send_ok 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bvgpu-apply-host-resolution.ps1 -Width 1600 -Height 900 -LaunchPointerTarget' || fail 'guest resize failed'
for _ in $(seq 1 120); do
  grep '"name":"SET_SCANOUT"' "$RUN/virtio-gpu.jsonl" 2>/dev/null | grep '"response_name":"OK_NODATA"' | grep -q '"rect_w":1600,"rect_h":900' && break
  sleep 1
done
grep '"name":"SET_SCANOUT"' "$RUN/virtio-gpu.jsonl" | grep '"response_name":"OK_NODATA"' | grep -q '"rect_w":1600,"rect_h":900' || fail 'active 1600x900 scanout absent'
printf '%s\n' 'POINTER move:16384x16384' >> "$INPUT"
for _ in $(seq 1 120); do grep -q '^BVTARGET ready width=1600 height=900 center_x=800 center_y=450' "$RUN/share/bv-pointer-target-ready.log" 2>/dev/null && break; sleep 1; done
grep -q '^BVTARGET ready width=1600 height=900 center_x=800 center_y=450 hwnd=[1-9][0-9]*' "$RUN/share/bv-pointer-target-ready.log" || fail 'target not ready'
echo 'B4 pointer target ready: width=1600 height=900 center_x=800 center_y=450' >> "$RUN/run.log"
send_ok 'powershell -NoProfile -Command "Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = '\''cmd /c powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMPtr\bv-pointer-capture.ps1 -DurationMs 360000 > C:\BridgeVMPtr\bvptr.log 2>&1'\'' } | Out-Null; Write-Output BVPTR_LAUNCHED"' || fail 'probe launch failed'
for _ in $(seq 1 600); do
  grep -q 'BVPTR summary' "$RUN/share/bvptr.log" 2>/dev/null && break
  kill -0 "$pid" 2>/dev/null || break; sleep 1
done
grep -q 'BVPTR summary' "$RUN/share/bvptr.log" || fail 'probe summary absent'
for _ in $(seq 1 30); do [[ -s "$RUN/share/bv-pointer-target-click.log" ]] && break; sleep 1; done
