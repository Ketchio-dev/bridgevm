#!/usr/bin/env bash
# Drive F1-F4 against one prepared Windows clone and retain machine-readable proof.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT=""
TARGET=""
VARS=""
BINARY=""
VIOGPU_DIR=""
MOLTENVK=""
WATCHDOG_MS=900000
STEP_TIMEOUT=120
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --vars) VARS="$2"; shift 2 ;;
    --binary) BINARY="$2"; shift 2 ;;
    --viogpu-dir) VIOGPU_DIR="$2"; shift 2 ;;
    --moltenvk) MOLTENVK="$2"; shift 2 ;;
    --watchdog-ms) WATCHDOG_MS="$2"; shift 2 ;;
    *) echo "unknown closure-interact option: $1" >&2; exit 2 ;;
  esac
done
for required in OUT TARGET VARS BINARY VIOGPU_DIR MOLTENVK; do
  [[ -n "${!required}" ]] || { echo "missing --${required,,}" >&2; exit 2; }
done
mkdir -p "$OUT/share" "$OUT/captures"
CTL="$OUT/agent.ctl"; : > "$CTL"
INPUT="$OUT/input.ctl"; : > "$INPUT"
RUN_LOG="$OUT/run.log"
cp "$REPO/scripts/win-assets/bvgpu-apply-host-resolution.ps1" "$OUT/share/"
cp "$REPO/scripts/win-assets/bv-windows-closure-proof.ps1" "$OUT/share/"

wait_for() {
  local pattern="$1" count="$2" timeout="$3" observed
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    observed=$(grep -cE "$pattern" "$RUN_LOG" 2>/dev/null || true)
    (( observed >= count )) && return 0
    kill -0 "$LAUNCHER" 2>/dev/null || return 1
    sleep 0.25
  done
  return 1
}

send() {
  local command="$1" pattern="${2:-^BVAGENT END }" before
  before=$(grep -cE "$pattern" "$RUN_LOG" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$CTL"
  wait_for "$pattern" $((before + 1)) "$STEP_TIMEOUT" || {
    echo "FAIL: no reply for ${command:0:100}" >&2
    return 1
  }
}

send_ok() {
  local command="$1" before line
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG" 2>/dev/null || true)
  send "$command" '^BVAGENT END '
  line=$(grep -E '^BVAGENT CMD .* exit=' "$RUN_LOG" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$RUN_LOG") -gt $before && "$line" == *' exit=0' ]]
}

capture_active_scanout() {
  local label="$1" before line ppm
  before=$(grep -c "ramfb checkpoint: label=$label " "$RUN_LOG" 2>/dev/null || true)
  printf 'SNAPSHOT %s\n' "$label" >> "$INPUT"
  wait_for "ramfb checkpoint: label=$label state=captured " $((before + 1)) 30 || return 1
  line=$(grep "ramfb checkpoint: label=$label state=captured " "$RUN_LOG" | tail -1)
  ppm=$(sed -E 's/^.* ppm=//' <<<"$line")
  [[ "$(basename "$ppm")" == virtio-gpu-checkpoint-* && -s "$ppm" ]] || return 1
  cp "$ppm" "$OUT/captures/$label.ppm"
  shasum -a 256 "$OUT/captures/$label.ppm" > "$OUT/captures/$label.ppm.sha256"
}

cleanup() {
  local status=$?
  if [[ -n "${LAUNCHER:-}" ]] && kill -0 "$LAUNCHER" 2>/dev/null; then
    printf 'shutdown /s /t 0\n' >> "$CTL" 2>/dev/null || true
    for _ in $(seq 1 80); do kill -0 "$LAUNCHER" 2>/dev/null || break; sleep 0.25; done
    kill "$LAUNCHER" 2>/dev/null || true
    wait "$LAUNCHER" 2>/dev/null || true
  fi
  return "$status"
}
trap cleanup EXIT

BRIDGEVM_PREBUILT_PROBE="$BINARY" BRIDGEVM_VULKAN_LIB="$MOLTENVK" \
BRIDGEVM_BOOT_PROGRESS_KILL=1 \
"$REPO/scripts/run-hvf-windows-installed-boot.sh" \
  --target "$TARGET" --vars "$VARS" --evidence-dir "$OUT" \
  --watchdog-ms "$WATCHDOG_MS" --ram-mib 6144 --smp-cpus 4 --max-reboots 8 \
  --skip-build --release --enable-xhci --input-control "$INPUT" \
  --agent-service-control "$CTL" --agent-share-host "$OUT/share" \
  --agent-share-guest 'C:\BridgeVMClosure' --agent-share-ms 500 \
  --virtio-gpu-3d --gpu-trace "$OUT/virtio-gpu.jsonl" \
  --gpu-trace-protocol venus --trace-venus-start --viogpu3d-dir "$VIOGPU_DIR" \
  --display-export-ppm "$OUT/display-live.ppm" --display-export-fb "$OUT/display.fb" \
  --display-export-ms 100 > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!

wait_for '^BVAGENT SERVICE start' 1 600 || { echo 'FAIL: agent service timeout' >&2; exit 1; }
for file in bvgpu-apply-host-resolution.ps1 bv-windows-closure-proof.ps1; do
  bytes=$(stat -f %z "$OUT/share/$file")
  wait_for "^BVAGENT SHARE host->guest $file bytes=$bytes " 1 180 || {
    echo "FAIL: $file share timeout" >&2; exit 1;
  }
done

f1=fail; f2=fail; f3=fail; f4=blocked
F1_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-windows-closure-proof.ps1 -Action F1'
if send_ok "$F1_CMD" && grep -Eq '^BVF1 testsigning=True viogpu_status=OK viogpu_problem=0 vioserial_status=OK vioserial_problem=0 agent_sha256=[0-9a-f]{64}$' "$RUN_LOG" \
  && grep -Eq '^BVF1MODE .* modes=([2-9]|[1-9][0-9]+) has_1600x900=True ' "$RUN_LOG"; then
  f1=pass
fi

printf 'RESIZE 1600x900\n' >> "$INPUT"
wait_for '^live input accepted: resize=1600x900$' 1 30 || true
APPLY_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bvgpu-apply-host-resolution.ps1 -Width 1600 -Height 900'
send_ok "$APPLY_CMD" || true
DISPLAY_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-windows-closure-proof.ps1 -Action Display'
if send_ok "$DISPLAY_CMD" && grep -Eq '^BVF2 .* current=1600x900 .* modes=([2-9]|[1-9][0-9]+) has_1600x900=True$' "$RUN_LOG" \
  && grep '"name":"SET_SCANOUT"' "$OUT/virtio-gpu.jsonl" | grep '"response_name":"OK_NODATA"' | grep -q '"rect_w":1600,"rect_h":900'; then
  f2=pass
fi
LAUNCH_CMD='powershell -NoProfile -Command "Start-Process notepad.exe; Start-Sleep -Seconds 3; Write-Output BVNOTEPADSTARTED"'
send_ok "$LAUNCH_CMD" || true
send 'WINLIST' '^BVAGENT WINLIST WINEND$' || true
win_line=$(grep '^BVAGENT WINLIST WIN ' "$RUN_LOG" | while IFS= read -r line; do
  title_b64=$(awk '{print $11}' <<<"$line")
  title=$(printf '%s' "$title_b64" | base64 -D 2>/dev/null || true)
  [[ "$title" == *Notepad* ]] && { printf '%s\n' "$line"; break; }
done)
hwnd=$(awk '{print $4}' <<<"${win_line:-}")
if [[ "$hwnd" =~ ^[0-9]+$ ]]; then
  send "WINBOUNDS $hwnd 50 60 700 500" "^BVAGENT WINBOUNDS $hwnd 50 60 700 500 -> OK WINBOUNDS$" || true
  send "WINFOCUS $hwnd" "^BVAGENT WINFOCUS $hwnd -> OK WINFOCUS$" || true
  WINDOW_CMD="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-windows-closure-proof.ps1 -Action Window -Hwnd $hwnd"
  send_ok "$WINDOW_CMD" || true
  if grep -Eq "^BVWINDOW hwnd=$hwnd exists=True pid=[0-9]+ rect=50,60,700,500 foreground=$hwnd$" "$RUN_LOG"; then
    f3=partial
    printf 'KEY text-hex:427269646765564d20476c7970682050726f6265204142434445464748494a4b\n' >> "$INPUT"
    printf 'KEY text-hex:4c4d4e4f505152535455565758595a206162636465666768696a6b6c6d6e6f70\n' >> "$INPUT"
    printf 'KEY text-hex:7172737475767778797a20303132333435363738392021402324255e262a2829\n' >> "$INPUT"
    wait_for 'live input accepted: command=Key\(' 3 30 || true
    sleep 2
    if capture_active_scanout f4-notepad-focused; then
      ocr=$(tesseract "$OUT/captures/f4-notepad-focused.ppm" stdout 2>"$OUT/captures/f4-tesseract.err" | tr '\r\n' ' ')
      printf '%s\n' "$ocr" > "$OUT/captures/f4-ocr.txt"
      if grep -Eiq 'BridgeVM|Glyph|Probe|File|Edit|View|Notepad' "$OUT/captures/f4-ocr.txt"; then
        f4=measured-visible-text
      else
        f4=measured-no-recognized-glyphs
      fi
    fi
    send "WINCLOSE $hwnd" "^BVAGENT WINCLOSE $hwnd -> OK WINCLOSE$" || true
    sleep 2
    before_final_list=$(grep -c '^BVAGENT WINLIST WINEND$' "$RUN_LOG" 2>/dev/null || true)
    send 'WINLIST' '^BVAGENT WINLIST WINEND$' || true
    final_list=$(awk -v prior="$before_final_list" '
      /^BVAGENT WINLIST WINEND$/ { lists++; next }
      lists == prior { print }
    ' "$RUN_LOG")
    if ! grep -Eq "^BVAGENT WINLIST WIN $hwnd " <<<"$final_list"; then
      f3=pass
    fi
  fi
fi

printf 'shutdown /s /t 0\n' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true
LAUNCHER=''
{
  echo "f1_driver_load=$f1"
  echo "f2_resize=$f2"
  echo "f3_window_verbs=$f3"
  echo "f4_glyph_observation=$f4"
  echo 'requested=1600x900'
  echo "notepad_hwnd=${hwnd:-absent}"
  echo "active_scanout_capture=$([[ -s "$OUT/captures/f4-notepad-focused.ppm" ]] && echo present || echo absent)"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
[[ "$f1" == pass && "$f2" == pass && "$f3" == pass && "$f4" != blocked ]]
