#!/usr/bin/env bash
# Verifies A2: a real Vulkan title reports guest-side FPS samples with p50>=30.
# PPSSPP is a native Windows ARM64 application configured for Vulkan. Its own
# log is the source of frame-rate samples; host RESOURCE_FLUSH is DWM scanout
# and is deliberately not treated as application FPS.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/vulkan-fps-$(date +%Y%m%d-%H%M%S)}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-180}
TITLE_SECONDS=${TITLE_SECONDS:-30}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
GATE_SOURCE=$REPO/scripts/win-assets/bvgpu-real-title-gate.ps1
PPSSPP_SOURCE=${PPSSPP_SOURCE:-$HOME/BridgeVM/apps/ppsspp}

[[ -f "$TARGET" ]] || fail "target image missing: $TARGET"
[[ -f "$VARS" ]] || fail "vars missing: $VARS"
[[ -f "$GATE_SOURCE" ]] || fail "title gate missing: $GATE_SOURCE"
[[ -f "$PPSSPP_SOURCE/PPSSPPWindowsARM64.exe" ]] \
  || fail "PPSSPP ARM64 payload missing: $PPSSPP_SOURCE"

WORK=$HOME/BridgeVM/work/vulkan-fps-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

CTL=$OUT/agent.ctl
: > "$CTL"
RUN_LOG=$OUT/run.log
HOST_SHARE=$OUT/share-host
mkdir -p "$HOST_SHARE"
cp "$GATE_SOURCE" "$HOST_SHARE/bvgpu-real-title-gate.ps1"
# The known agent image has no title payload. Stage the existing 40 MiB native
# ARM64 PPSSPP tree through the already-proven recursive folder-share path,
# avoiding a new disk injector pass solely for an application payload.
cp -R "$PPSSPP_SOURCE" "$HOST_SHARE/ppsspp"

wait_for() { # pattern, count, timeout seconds
  local deadline=$((SECONDS + $3)) n
  while (( SECONDS < deadline )); do
    n=$(grep -cE "$1" "$RUN_LOG" 2>/dev/null || true)
    (( n >= $2 )) && return 0
    sleep 0.3
  done
  return 1
}

send() { # control line, completion pattern, optional timeout
  local before timeout=${3:-$STEP_TIMEOUT}
  before=$(grep -cE "$2" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$1" >> "$CTL"
  wait_for "$2" $((before + 1)) "$timeout" \
    || fail "no reply for: ${1:0:100}"
}

run_guest() { # command, timeout
  local before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$1" '^BVAGENT END ' "${2:-$STEP_TIMEOUT}"
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before ]] \
    || fail "command produced no CMD result"
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

wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" \
  || fail "agent never reached service state within ${BOOT_TIMEOUT}s"
echo "agent up at ${SECONDS}s"

# Wait for the host-share syncer and prove both the executable and current gate
# are visible before spending the measurement interval.
CHECK='powershell -NoProfile -Command "if ((Test-Path C:\BridgeVMShare\ppsspp\PPSSPPWindowsARM64.exe) -and (Test-Path C:\BridgeVMShare\bvgpu-real-title-gate.ps1)) { Write-Output assets=OK; exit 0 } else { Write-Output assets=MISSING; exit 2 }"'
for _ in $(seq 1 30); do
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$CHECK" '^BVAGENT END ' 30
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  if [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before ]] \
     && [[ "$line" == *" exit=0" ]] \
     && grep -q '^assets=OK\r\{0,1\}$' "$RUN_LOG"; then
    break
  fi
  sleep 1
done
grep -q '^assets=OK$' < <(tr -d '\r' < "$RUN_LOG") || fail "PPSSPP or synced gate missing"
echo "title assets ready at ${SECONDS}s"

GUEST_GATE='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMShare\bvgpu-real-title-gate.ps1 -Executable C:\BridgeVMShare\ppsspp\PPSSPPWindowsARM64.exe -MinimumSeconds '"$TITLE_SECONDS"
run_guest "$GUEST_GATE" $((TITLE_SECONDS + STEP_TIMEOUT))

FPS_LINE=$(tr -d '\r' < "$RUN_LOG" | grep 'guest_fps samples=' | tail -1)
SAMPLES=$(grep -oE 'samples=[0-9]+' <<< "$FPS_LINE" | cut -d= -f2)
P50=$(grep -oE 'p50=[0-9]+([.][0-9]+)?' <<< "$FPS_LINE" | cut -d= -f2)
TITLE_PASS=$(tr -d '\r' < "$RUN_LOG" | grep -c 'BVGPU-REAL-TITLE-PASS' || true)

A2=fail
if [[ "${SAMPLES:-0}" -gt 0 ]] \
   && awk -v p="${P50:-0}" 'BEGIN { exit !(p >= 30.0) }' \
   && [[ "$TITLE_PASS" -gt 0 ]]; then
  A2=pass
  echo "A2 Vulkan FPS: PASS (samples=$SAMPLES p50=$P50)"
else
  echo "A2 Vulkan FPS: FAIL (samples=${SAMPLES:-?} p50=${P50:-?} title_pass=$TITLE_PASS)" >&2
  echo "  line: ${FPS_LINE:-<none>}" >&2
fi

printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true

{
  echo "out_dir=$OUT"
  echo "a2_vulkan_fps=$A2"
  echo "samples=${SAMPLES:-}"
  echo "p50=${P50:-}"
  echo "fps_line=${FPS_LINE:-}"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$A2" == pass ]]
