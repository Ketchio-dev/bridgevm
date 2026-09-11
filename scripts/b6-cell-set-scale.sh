#!/usr/bin/env bash
# Stage 1 of the B6 glyph-matrix cell harness: boot once, set resolution and
# scale, then shut down cleanly. A SECOND, separate launcher process (
# cell-capture.sh) boots the same disk to do the actual captures, because
# two independent live spikes (this session's b6-scale-mechanism-spike and
# this harness's own first attempt) both showed that capturing via
# scripts/capture-active-iosurface.py after an in-session guest restart
# always fails with "active IOSurface seed did not advance", regardless of
# any subsequent content-changing action -- the display-export/IOSurface
# pipeline does not recover from a guest-triggered reboot inside the same
# host launcher process. Rebooting between two separate launcher processes,
# with the disk persisting the registry change, avoids the defect entirely.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT=""; TARGET=""; VARS=""; BINARY=""; VIOGPU_DIR=""; MOLTENVK=""
WIDTH=""; HEIGHT=""; LOGPIXELS=""
WATCHDOG_MS=3000000; STEP_TIMEOUT=120; AGENT_TIMEOUT=2700
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --vars) VARS="$2"; shift 2 ;;
    --binary) BINARY="$2"; shift 2 ;;
    --viogpu-dir) VIOGPU_DIR="$2"; shift 2 ;;
    --moltenvk) MOLTENVK="$2"; shift 2 ;;
    --width) WIDTH="$2"; shift 2 ;;
    --height) HEIGHT="$2"; shift 2 ;;
    --logpixels) LOGPIXELS="$2"; shift 2 ;;
    *) echo "unknown cell-set-scale option: $1" >&2; exit 2 ;;
  esac
done
for required in OUT TARGET VARS BINARY VIOGPU_DIR MOLTENVK WIDTH HEIGHT LOGPIXELS; do
  [[ -n "${!required}" ]] || { echo "missing --${required,,}" >&2; exit 2; }; done
mkdir -p "$OUT/share"; RUN_LOG="$OUT/run.log"
CTL="$OUT/agent.ctl"; : > "$CTL"; INPUT="$OUT/input.ctl"; : > "$INPUT"
cp "$REPO/scripts/win-assets/bvgpu-apply-host-resolution.ps1" \
   "$REPO/scripts/win-assets/bv-windows-closure-proof.ps1" \
   "$REPO/scripts/win-assets/bv-b6-set-scale.ps1" \
   "$REPO/scripts/win-assets/bv-b6-read-logpixels.ps1" \
   "$OUT/share/"

# Agent control-channel helpers, shared with the other scripts that drive it.
source "$REPO/scripts/agent-channel-lib.sh"





cleanup() {
  local status=$?
  if [[ -n "${LAUNCHER:-}" ]] && kill -0 "$LAUNCHER" 2>/dev/null; then
    for _ in $(seq 1 240); do kill -0 "$LAUNCHER" 2>/dev/null || break; sleep 0.5; done
    kill "$LAUNCHER" 2>/dev/null || true
  fi
  wait "$LAUNCHER" 2>/dev/null || true
  return "$status"
}
trap cleanup EXIT

BRIDGEVM_PREBUILT_PROBE="$BINARY" BRIDGEVM_VULKAN_LIB="$MOLTENVK" \
BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT=1 BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT=0 BRIDGEVM_VIRTIO_GPU_ASYNC_PRESENT=0 \
BRIDGEVM_BOOT_PROGRESS_KILL=1 \
"$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$TARGET" --vars "$VARS" --evidence-dir "$OUT" \
  --watchdog-ms "$WATCHDOG_MS" --ram-mib 6144 --smp-cpus 4 --max-reboots 8 \
  --skip-build --release --enable-xhci --input-control "$INPUT" \
  --agent-service-control "$CTL" --agent-share-host "$OUT/share" \
  --agent-share-guest 'C:\BridgeVMClosure' --agent-share-ms 500 \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --trace-venus-start --viogpu3d-dir "$VIOGPU_DIR" \
  > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!

wait_for '^BVAGENT SERVICE start' 1 "$AGENT_TIMEOUT" || { echo 'FAIL: agent service timeout' >&2; exit 1; }
for file in bvgpu-apply-host-resolution.ps1 bv-windows-closure-proof.ps1 bv-b6-set-scale.ps1 bv-b6-read-logpixels.ps1; do
  bytes=$(stat -f %z "$OUT/share/$file")
  wait_for "^BVAGENT SHARE host->guest $file bytes=$bytes " 1 300 \
    || { echo "FAIL: $file share timeout" >&2; exit 1; }
done
wait_firstboot || { echo 'FAIL: firstboot stage4 readiness timeout' >&2; exit 1; }

f1=fail; F1_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-windows-closure-proof.ps1 -Action F1'
if send_ok "$F1_CMD" && grep -Eq '^BVF1 testsigning=True viogpu_status=OK viogpu_problem=0 vioserial_status=OK vioserial_problem=0 agent_sha256=[0-9a-f]{64}\r?$' "$RUN_LOG"; then
  f1=pass
fi

printf 'RESIZE %sx%s\n' "$WIDTH" "$HEIGHT" >> "$INPUT"; wait_for "^live input accepted: resize=${WIDTH}x${HEIGHT}\$" 1 30 || true
APPLY_CMD="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bvgpu-apply-host-resolution.ps1 -Width $WIDTH -Height $HEIGHT"
send_ok "$APPLY_CMD" || true
source "$REPO/scripts/b6-display-proof.sh"
f2=fail
if b6_display_matches "$WIDTH" "$HEIGHT"; then
  f2=pass
fi

BEFORE_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-b6-read-logpixels.ps1'
before_logpixels="unknown"
if send_ok "$BEFORE_CMD"; then
  before_logpixels=$(grep -E '^BVLOGPIXELS value=' "$RUN_LOG" | tail -1 | tr -d '\r')
fi

scale_set=fail
SCALE_CMD="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-set-scale.ps1 -LogPixels $LOGPIXELS"
if send_ok "$SCALE_CMD" && grep -Eq "^BVSCALESET logpixels=$LOGPIXELS win8dpiscaling=1\r?\$" "$RUN_LOG"; then
  scale_set=pass
fi

# A clean shutdown (not /f), so Windows commits the registry write to disk
# through its own normal flush path before the launcher exits.
printf 'shutdown /s /t 0\n' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true; LAUNCHER=''
{
  echo "width=$WIDTH"; echo "height=$HEIGHT"; echo "logpixels=$LOGPIXELS"
  echo "f1_driver_load=$f1"; echo "f2_resize=$f2"; echo "scale_set=$scale_set"
  echo "before_logpixels=$before_logpixels"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$f1" == pass && "$f2" == pass && "$scale_set" == pass ]]
