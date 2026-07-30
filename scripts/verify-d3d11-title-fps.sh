#!/usr/bin/env bash
# Verifies A3: an ARM64 D3D11/DXVK draw+present workload reports guest-side
# FPS samples with p50>=30. The executable validates its rendered backbuffer,
# presents 900 frames, and times 30-frame windows with QueryPerformanceCounter.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/d3d11-fps-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-300}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
DXVK_ARM64=${DXVK_ARM64:-$HOME/BridgeVM/dxvk/build.arm64/src}

[[ -f "$TARGET" && -f "$VARS" ]] || fail "target/vars missing"
[[ -f "$DXVK_ARM64/d3d11/d3d11.dll" ]] || fail "ARM64 DXVK d3d11.dll missing"
[[ -f "$DXVK_ARM64/dxgi/dxgi.dll" ]] || fail "ARM64 DXVK dxgi.dll missing"

WORK=$HOME/BridgeVM/work/d3d11-fps-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

HOST_SHARE=$OUT/share-host
mkdir -p "$HOST_SHARE"
zig cc -target aarch64-windows-gnu -O2 -Wall -Wextra -Werror \
  -o "$HOST_SHARE/bridgevm-d3d11-present-fps.exe" \
  scripts/win-tests/bridgevm-d3d11-present-smoke.c -ld3d11 -ldxgi
cp "$DXVK_ARM64/d3d11/d3d11.dll" "$HOST_SHARE/d3d11.dll"
cp "$DXVK_ARM64/dxgi/dxgi.dll" "$HOST_SHARE/dxgi.dll"

CTL=$OUT/agent.ctl
: > "$CTL"
RUN_LOG=$OUT/run.log

wait_for() {
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(grep -cE "$1" "$RUN_LOG" 2>/dev/null || true)
    (( n >= $2 )) && return 0
    sleep 0.3
  done
  return 1
}

send() {
  local before timeout=${3:-$STEP_TIMEOUT}
  before=$(grep -cE "$2" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$1" >> "$CTL"
  wait_for "$2" $((before + 1)) "$timeout" || fail "no reply for: ${1:0:100}"
}

run_guest() {
  local before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$1" '^BVAGENT END ' "${2:-$STEP_TIMEOUT}"
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before ]] \
    || fail "command produced no result"
  [[ "$line" == *" exit=0" ]] || fail "guest command failed: ${line:0:180}"
}

scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 \
  --agent-service-control "$CTL" \
  --agent-share-host "$HOST_SHARE" --agent-share-guest 'C:\BridgeVMShare' \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'kill $LAUNCHER 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" || fail "agent service timeout"
echo "agent up at ${SECONDS}s"

for file in bridgevm-d3d11-present-fps.exe d3d11.dll dxgi.dll; do
  bytes=$(stat -f %z "$HOST_SHARE/$file")
  wait_for "^BVAGENT SHARE host->guest $file bytes=$bytes " 1 300 \
    || fail "$file did not synchronize"
done
echo "D3D11 payload ready at ${SECONDS}s"

# Keep the workload beside DXVK so normal DLL search loads the intended ARM64
# d3d11.dll and dxgi.dll. The module path printed by the executable is checked
# below, so a silent fallback to Microsoft's D3D11 cannot pass.
CMD='set "BV_PRESENT_DEMO=1" && set "VK_DRIVER_FILES=C:\BridgeVM\viogpu3d\virtio_icd.arm64.json" && set "DXVK_LOG_LEVEL=info" && set "DXVK_LOG_PATH=C:\BridgeVMShare" && cd /d C:\BridgeVMShare && bridgevm-d3d11-present-fps.exe'
run_guest "$CMD" 600

LOG=$(tr -d '\r' < "$RUN_LOG")
FPS_LINE=$(grep '^BV-D3D11-PRESENT-FPS samples=' <<< "$LOG" | tail -1)
SAMPLES=$(grep -oE 'samples=[0-9]+' <<< "$FPS_LINE" | cut -d= -f2)
P50=$(grep -oE 'p50=[0-9]+([.][0-9]+)?' <<< "$FPS_LINE" | cut -d= -f2)
MODULE_LINE=$(grep '^BV-D3D11-PRESENT-MODULE ' <<< "$LOG" | tail -1)
BACKBUFFER_LINE=$(grep '^BV-D3D11-PRESENT-BACKBUFFER ' <<< "$LOG" | tail -1)
PASS_COUNT=$(grep -c '^BV-D3D11-PRESENT-PASS$' <<< "$LOG" || true)

A3=fail
if [[ "${SAMPLES:-0}" -gt 0 ]] \
   && awk -v p="${P50:-0}" 'BEGIN { exit !(p >= 30.0) }' \
   && [[ "$MODULE_LINE" == *'BridgeVMShare\d3d11.dll'* ]] \
   && [[ "$BACKBUFFER_LINE" == *'bad_pixels=0'* ]] \
   && [[ "$PASS_COUNT" -gt 0 ]]; then
  A3=pass
  echo "A3 D3D11 FPS: PASS (samples=$SAMPLES p50=$P50)"
else
  echo "A3 D3D11 FPS: FAIL (samples=${SAMPLES:-?} p50=${P50:-?} pass=$PASS_COUNT)" >&2
  echo "  module: ${MODULE_LINE:-<none>}" >&2
  echo "  backbuffer: ${BACKBUFFER_LINE:-<none>}" >&2
fi

printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true
{
  echo "out_dir=$OUT"
  echo "a3_d3d11_fps=$A3"
  echo "samples=${SAMPLES:-}"
  echo "p50=${P50:-}"
  echo "fps_line=${FPS_LINE:-}"
  echo "module_line=${MODULE_LINE:-}"
  echo "backbuffer_line=${BACKBUFFER_LINE:-}"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$A3" == pass ]]
