#!/usr/bin/env bash
# Measures B4 host-click -> guest framebuffer response latency.
# On the 800x600 Windows desktop, absolute HID coordinate 820x31650 targets the
# Start button. The probe captures before/after and bounded delayed frames; the
# first checksum change after injection is a measured upper bound.
set -euo pipefail
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd); cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }
TARGET=${TARGET:-$HOME/BridgeVM/work/net-live-20260724.raw}
VARS=${VARS:-$HOME/BridgeVM/work/net-live-20260724-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/pointer-latency-$(date +%Y%m%d-%H%M%S)}
VIOGPU_DIR=${VIOGPU3D_DIR:-$HOME/BridgeVM/work/download-120.45-backing-only}
WORK=$HOME/BridgeVM/work/pointer-latency
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"; cp -c "$TARGET" "$WORK/disk.raw"; cp "$VARS" "$WORK/vars.fd"
CTL=$OUT/agent.ctl; : > "$CTL"

scripts/run-hvf-windows-installed-boot.sh \
  --target "$WORK/disk.raw" --vars "$WORK/vars.fd" --evidence-dir "$OUT" \
  --watchdog-ms 600000 --ram-mib 6144 --smp-cpus 4 --enable-xhci \
  --pointer-input-actions 'click:820x31650' \
  --pointer-input-fire-delay-ms 150000 \
  --pointer-input-ramfb-delay-ms '5,15,30,60,120,250,500,1000' \
  --agent-service-control "$CTL" \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
PID=$!; trap 'kill $PID 2>/dev/null || true; pkill -f hvf_gic_boot_probe 2>/dev/null || true' EXIT

for _ in $(seq 1 240); do
  grep -q '^xHCI pointer-input injection .* fired:' "$OUT/run.log" 2>/dev/null && break
  sleep 1
done
grep -q '^xHCI pointer-input injection .* fired:' "$OUT/run.log" || fail "click did not fire"
sleep 3
printf '%s\n' 'shutdown /s /t 3' >> "$CTL"
wait "$PID" 2>/dev/null || true

# Parse checkpoint lines from the run log. before is the pre-click baseline;
# delay-Nms is sampled after the click. A changed checksum means the guest's
# composited desktop reaction reached host-visible framebuffer publication.
BASE=$(tr -d '\r' < "$OUT/run.log" | sed -n 's/.*label=pointer-input-before .*checksum64=\([^ ]*\).*/\1/p' | tail -1)
[[ -n "$BASE" ]] || fail "no before checksum"
FIRST=''
{
  echo "baseline=$BASE"
  for ms in 5 15 30 60 120 250 500 1000; do
    sum=$(tr -d '\r' < "$OUT/run.log" | sed -n "s/.*label=pointer-input-delay-${ms}ms .*checksum64=\([^ ]*\).*/\1/p" | tail -1)
    changed=false
    [[ -n "$sum" && "$sum" != "$BASE" ]] && changed=true
    [[ -z "$FIRST" && "$changed" == true ]] && FIRST=$ms
    echo "delay_${ms}ms_checksum=${sum:-missing} changed=$changed"
  done
  echo "first_changed_ms=${FIRST:-none}"
} > "$OUT/latency.txt"
cat "$OUT/latency.txt"
[[ -n "$FIRST" ]] || fail "no visible guest reaction within 1000ms"
echo "B4 pointer latency: first visible response <= ${FIRST}ms"
