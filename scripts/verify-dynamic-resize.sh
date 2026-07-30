#!/usr/bin/env bash
# Verifies A8: a live host resize request is accepted and Windows reports the
# exact requested resolution after the virtio-gpu display/config event.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/resize-verify-$(date +%Y%m%d-%H%M%S)}
REQUEST=${REQUEST:-1600x900}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-90}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
[[ "$REQUEST" =~ ^[0-9]+x[0-9]+$ ]] || fail "REQUEST must be WIDTHxHEIGHT"

WORK=$HOME/BridgeVM/work/resize-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"
CTL=$OUT/agent.ctl; : > "$CTL"
INPUT=$OUT/input.ctl; : > "$INPUT"
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
  local before; before=$(grep -cE "$2" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$1" >> "$CTL"
  wait_for "$2" $((before + 1)) "$STEP_TIMEOUT" || fail "no reply for: ${1:0:90}"
}
query_resolution() {
  local before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  # Win32_VideoController reports null CurrentHorizontal/VerticalResolution for
  # this WDDM driver (the first live run returned just "x"). Query the actual
  # interactive desktop bounds instead; this is what applications observe.
  send 'powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; Write-Output (\"guest_resolution={0}x{1}\" -f $b.Width,$b.Height)"' '^BVAGENT END '
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before && "$line" == *' exit=0' ]] \
    || fail "guest resolution query failed"
  tr -d '\r' < "$RUN_LOG" | grep '^guest_resolution=' | tail -1 | cut -d= -f2
}

scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --evidence-dir "$OUT" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
  --ram-mib 6144 --smp-cpus 4 --enable-xhci --input-control "$INPUT" \
  --agent-service-control "$CTL" \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" --trace-venus-start \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!
trap 'kill $LAUNCHER 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT
wait_for '^BVAGENT SERVICE start' 1 "$BOOT_TIMEOUT" || fail "agent service timeout"
echo "agent up at ${SECONDS}s"

driver_before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
send 'powershell -NoProfile -Command "Get-CimInstance Win32_VideoController | Format-List Name,Status,ConfigManagerErrorCode,PNPDeviceID,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution; Get-PnpDevice -Class Display | Format-List FriendlyName,Status,Problem,InstanceId"' '^BVAGENT END '
driver_line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
[[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $driver_before && "$driver_line" == *' exit=0' ]] \
  || fail "guest display-driver query failed"
BEFORE=$(query_resolution)
echo "guest before: $BEFORE"
printf 'RESIZE %s\n' "$REQUEST" >> "$INPUT"
wait_for "^live input accepted: resize=$REQUEST$" 1 30 \
  || fail "host did not accept resize=$REQUEST"
echo "host accepted resize=$REQUEST"

AFTER=''
for _ in $(seq 1 30); do
  AFTER=$(query_resolution)
  [[ "$AFTER" == "$REQUEST" ]] && break
  sleep 2
done

A8=fail
if [[ "$AFTER" == "$REQUEST" ]]; then
  A8=pass
  echo "A8 resize: PASS (guest=$AFTER)"
else
  echo "A8 resize: FAIL (requested=$REQUEST guest=${AFTER:-?})" >&2
fi

printf '%s\n' 'shutdown /s /t 3' >> "$CTL"; wait "$LAUNCHER" 2>/dev/null || true
{
  echo "out_dir=$OUT"
  echo "a8_resize=$A8"
  echo "before=$BEFORE"
  echo "requested=$REQUEST"
  echo "after=$AFTER"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$A8" == pass ]]
