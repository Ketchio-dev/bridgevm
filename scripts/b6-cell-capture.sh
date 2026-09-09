#!/usr/bin/env bash
# Stage 2 of the B6 glyph-matrix cell harness. Boots the SAME disk
# cell-set-scale.sh already set resolution and scale on (never the same
# launcher process -- see that script's header for why an in-session guest
# reboot poisons scripts/capture-active-iosurface.py for the rest of the
# session). For 3 independent runs, captures both declared scenes (classic
# Notepad caption+menu, packaged Notepad tab+menu) plus effective DPI and a
# PresentMon frame-time sample. Not a shipped closure gate; evidence is
# recorded by the caller into a dated markdown doc, same as every other B6
# spike this session.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT=""; TARGET=""; VARS=""; BINARY=""; VIOGPU_DIR=""; MOLTENVK=""
WIDTH=""; HEIGHT=""; LOGPIXELS=""; PRESENTMON=""
WATCHDOG_MS=5400000; STEP_TIMEOUT=120; AGENT_TIMEOUT=2700
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
    --presentmon) PRESENTMON="$2"; shift 2 ;;
    *) echo "unknown cell-harness option: $1" >&2; exit 2 ;;
  esac
done
for required in OUT TARGET VARS BINARY VIOGPU_DIR MOLTENVK WIDTH HEIGHT LOGPIXELS PRESENTMON; do
  [[ -n "${!required}" ]] || { echo "missing --${required,,}" >&2; exit 2; }; done
mkdir -p "$OUT/share" "$OUT/captures"; RUN_LOG="$OUT/run.log"
CTL="$OUT/agent.ctl"; : > "$CTL"; INPUT="$OUT/input.ctl"; : > "$INPUT"
cp "$REPO/scripts/win-assets/bvgpu-apply-host-resolution.ps1" \
   "$REPO/scripts/win-assets/bv-windows-closure-proof.ps1" \
   "$REPO/scripts/win-assets/bv-windows-closure-launch.ps1" \
   "$REPO/scripts/win-assets/bv-windows-closure-discard.ps1" \
   "$REPO/scripts/win-assets/bv-b6-window-dpi.ps1" \
   "$REPO/scripts/win-assets/bv-b6-read-logpixels.ps1" \
   "$REPO/scripts/win-assets/bv-b6-modern-notepad-launch.ps1" \
   "$REPO/scripts/win-assets/bv-b6-modern-notepad-reset.ps1" \
   "$REPO/scripts/win-assets/bv-b6-presentmon-capture.ps1" \
   "$PRESENTMON" \
   "$OUT/share/"
PRESENTMON_NAME="$(basename "$PRESENTMON")"

# Agent control-channel helpers, shared with the other scripts that drive it.
source "$REPO/scripts/agent-channel-lib.sh"


# Baseline count of a pattern, taken before some action; call wait_after with
# the same pattern to block until a strictly new match appears. An absolute
# count is wrong here: firstboot's own reboots and earlier runs' typing keep
# advancing generic patterns like "SERVICE start" or "command=Key(", so an
# absolute ">= N" check can already be satisfied before the action it is
# meant to confirm ever happens.
wait_baseline() { grep -cE "$1" "$RUN_LOG" 2>/dev/null || true; }
wait_after() {
  local pattern="$1" baseline="$2" timeout="$3"
  wait_for "$pattern" "$((baseline + 1))" "$timeout"
}




capture_active_scanout() {
  local label="$1" capture="$OUT/captures/$1"
  python3 "$REPO/scripts/capture-active-iosurface.py" --iosurface "$OUT/display.fb.iosurface" \
    --out "$capture" --timeout-ms 5000 || return 1
  cp "$capture/presented.ppm" "$OUT/captures/$label.ppm" || return 1
  shasum -a 256 "$OUT/captures/$label.ppm" > "$OUT/captures/$label.ppm.sha256"
}

wait_second_boot() {
  # After a restart, a fresh BVAGENT SERVICE start must appear strictly after
  # the given baseline count -- firstboot's own internal reboots already
  # advance this counter, so an absolute ">= 2" check can be satisfied before
  # the restart this function is meant to wait for ever happens.
  local baseline="$1"
  wait_for '^BVAGENT SERVICE start' "$((baseline + 1))" "$AGENT_TIMEOUT"
}

wait_window_gone() {
  local hwnd="$1" attempt before
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    before=$(wc -l < "$RUN_LOG")
    send 'WINLIST' '^BVAGENT WINLIST WINEND$' || return 1
    if ! tail -n "+$((before + 1))" "$RUN_LOG" | grep -q "^BVAGENT WINLIST WIN $hwnd "; then
      return 0
    fi
    sleep 2
  done
  echo "FAIL: hwnd=$hwnd still listed after teardown" >&2
  return 1
}

find_hwnd() {
  local title_substr="$1" win_line before
  # Only the reply to *this* WINLIST counts. Scanning the whole log matched
  # windows listed earlier in the boot: on 2026-09-09 run 3's classic scene got
  # the packaged window's hwnd and run 2's packaged scene got the classic one,
  # so scenes typed into each other and a capture labelled classic-run3 is
  # actually the packaged window with three rounds of text accumulated in it.
  before=$(wc -l < "$RUN_LOG")
  send 'WINLIST' '^BVAGENT WINLIST WINEND$' || return 1
  win_line=$(tail -n "+$((before + 1))" "$RUN_LOG" | grep '^BVAGENT WINLIST WIN ' | while IFS= read -r line; do
    local title_b64 title
    title_b64=$(awk '{print $10}' <<<"$line")
    title=$(printf '%s' "$title_b64" | base64 -D 2>/dev/null || true)
    [[ "$title" == *"$title_substr"* ]] && { printf '%s\n' "$line"; }
  done | tail -1 || true)
  awk '{print $4}' <<<"${win_line:-}"
}

# One typing payload per run index so each of the 3 runs is independent
# content, not a repeated capture of identical bytes.
typing_hex_for_run() {
  case "$1" in
    1) printf '427269646765564d2050726f62652072756e31204142434445464748\n494a4b4c4d4e4f505152535455565758595a20616263646566676869\n6a6b6c6d6e6f707172737475767778797a2030313233343536373839\n' ;;
    2) printf '427269646765564d2050726f62652072756e32207a79787776757473\n7271706f6e6d6c6b6a696867666564636261205a5958575655545352\n51504f4e4d4c4b4a4948474645444342412039383736353433326162\n' ;;
    *) printf '427269646765564d2050726f62652072756e33204d4958454463617365\n30313233343536373839206162636465666768696a6b6c6d6e6f70\n7172737475767778797a4142434445464748494a4b4c4d4e4f505152\n' ;;
  esac
}

# Packaged Notepad renames its tab on the first space and its frame hwnd
# changes between runs; every packaged run has landed exactly the nine
# characters up to and including that space, while classic Notepad takes all
# 84 from the same chunks. This payload has no spaces so the scene can be
# measured while that behaviour is characterised separately.
packaged_typing_hex_for_run() {
  case "$1" in
    1) printf '427269646765564d2d7031414243444546474849\n4a4b4c4d4e4f5051525354555657\n58595a30313233343536373839\n' ;;
    2) printf '427269646765564d2d7032617a62796378647765\n7666756774687369726a716b\n706c6f6d6e30393138323733\n' ;;
    *) printf '427269646765564d2d7033516e5772457459754f\n69506c614b6a486766446453\n5a78436d566e42763132333435\n' ;;
  esac
}

cleanup() {
  local status=$?
  if [[ -n "${LAUNCHER:-}" ]] && kill -0 "$LAUNCHER" 2>/dev/null; then
    printf 'shutdown /s /f /t 0\n' >> "$CTL" 2>/dev/null || true
    for _ in $(seq 1 480); do kill -0 "$LAUNCHER" 2>/dev/null || break; sleep 0.5; done
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
  --display-export-ppm "$OUT/display-live.ppm" --display-export-fb "$OUT/display.fb" \
  --display-export-ms 100 > "$OUT/launcher.out" 2>&1 &
LAUNCHER=$!

wait_for '^BVAGENT SERVICE start' 1 "$AGENT_TIMEOUT" || { echo 'FAIL: agent service timeout' >&2; exit 1; }
for file in bvgpu-apply-host-resolution.ps1 bv-windows-closure-proof.ps1 bv-windows-closure-launch.ps1 \
            bv-windows-closure-discard.ps1 bv-b6-window-dpi.ps1 bv-b6-read-logpixels.ps1 \
            bv-b6-modern-notepad-reset.ps1 \
            bv-b6-modern-notepad-launch.ps1 bv-b6-presentmon-capture.ps1 "$PRESENTMON_NAME"; do
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
DISPLAY_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-windows-closure-proof.ps1 -Action Display'
f2=fail
if send_ok "$DISPLAY_CMD" && grep -Eq "^BVF2 .* current=${WIDTH}x${HEIGHT} modes=([2-9]|[1-9][0-9]+) has_${WIDTH}x${HEIGHT}=True\r?\$" "$RUN_LOG"; then
  f2=pass
fi

scale_set=fail
READ_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-b6-read-logpixels.ps1'
if send_ok "$READ_CMD" && grep -Eq "^BVLOGPIXELS value=$LOGPIXELS\r?\$" "$RUN_LOG"; then
  scale_set=pass
fi

runs_json="$OUT/runs.json"; printf '[]' > "$runs_json"
# 2026-09-09: WINFOCUS used to be sent with `|| true`, so a refused foreground
# was discarded and the run typed into a window that never became active. That
# is the whole of the "second capture in a boot fails" bug: no keys land, the
# scanout is never invalidated, and capture-active-iosurface.py times out on a
# seed that was never going to advance. See docs/windows-arm/evidence/
# b6-second-capture-focus-refusal-20260909.md. Focus is now a precondition:
# retried, verified against the guest's own GetForegroundWindow, and fatal to
# the scene when it never takes.
# POINTER coordinates are HID absolute-axis units spanning 0..0x7fff across the
# screen (POINTER_INPUT_AXIS_MAX in the probe's xhci_hid_input), not pixels. The
# proven pointer campaign converts with px * 32767 / (dimension - 1)
# (scripts/run-pointer-click-reliability-case.sh). Feeding raw pixels put every
# B6 click in the top-left corner -- 547x303 lands at about (27, 8) px -- which
# is why the packaged scene's first-run tip was never dismissed across twelve
# attempts and its captures always failed on an unchanged screen. Take pixels
# here and convert, so the units cannot be confused again.
hid_point() {
  local px="$1" py="$2"
  printf '%sx%s' "$(( px * 32767 / (WIDTH - 1) ))" "$(( py * 32767 / (HEIGHT - 1) ))"
}

foreground_hwnd() {
  local ps b64
  ps='Add-Type -Name Fg -Namespace Bv -MemberDefinition '"'"'[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow();'"'"'; Write-Output ("BVFOREGROUND " + [Bv.Fg]::GetForegroundWindow().ToInt64())'
  b64=$(printf '%s' "$ps" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n')
  send_ok "powershell -NoProfile -EncodedCommand $b64" || return 1
  grep -E '^BVFOREGROUND [0-9-]+' "$RUN_LOG" | tail -1 | awk '{print $2}' | tr -d '\r'
}

focus_window() {
  local hwnd="$1" click="${2:-}" attempt fg
  local reply
  for attempt in 1 2 3 4 5; do
    # Match either reply. Waiting only for OK made a prompt ERR burn the whole
    # STEP_TIMEOUT (120 s) per attempt -- 10 minutes per abandoned scene, live
    # on 2026-09-09 -- and reported it as "no reply" when the agent had in fact
    # answered immediately.
    if send "WINFOCUS $hwnd" "^BVAGENT WINFOCUS $hwnd -> (OK|ERR) WINFOCUS$"; then
      reply=$(grep -oE "BVAGENT WINFOCUS $hwnd -> (OK|ERR) WINFOCUS" "$RUN_LOG" | tail -1)
      if [[ "$reply" != *"-> OK"* ]]; then
        echo "FOCUS: hwnd=$hwnd refused by SetForegroundWindow (attempt $attempt)" >&2
        if [[ -n "$click" ]]; then
          # 2026-09-09: classic Notepad is refused the foreground on every run
          # after a packaged (UWP) scene has run, 5/5 both times. A click is
          # how a user takes focus and does not go through SetForegroundWindow.
          echo "FOCUS: hwnd=$hwnd trying pointer click at $click" >&2
          _b_fp=$(wait_baseline 'live input accepted: command=Pointer\(')
          printf 'POINTER click:%s\n' "$click" >> "$INPUT"
          wait_after 'live input accepted: command=Pointer\(' "$_b_fp" 15 || true
          sleep 1
          fg=$(foreground_hwnd || true)
          if [[ "$fg" == "$hwnd" ]]; then
            echo "FOCUS: hwnd=$hwnd took foreground from the click on attempt $attempt" >&2
            return 0
          fi
          echo "FOCUS: hwnd=$hwnd still not foreground after click (foreground=${fg:-unknown})" >&2
        fi
        sleep 2
        continue
      fi
      # SetForegroundWindow returning nonzero is not proof the foreground moved;
      # the agent reports only that return value (bvagent.ps1 WINFOCUS).
      fg=$(foreground_hwnd || true)
      if [[ "$fg" == "$hwnd" ]]; then
        echo "FOCUS: hwnd=$hwnd confirmed foreground on attempt $attempt" >&2
        return 0
      fi
      echo "FOCUS: hwnd=$hwnd reported OK but foreground=${fg:-unknown} (attempt $attempt)" >&2
    else
      echo "FOCUS: hwnd=$hwnd no reply within step timeout (attempt $attempt)" >&2
    fi
    sleep 2
  done
  echo "FAIL: hwnd=$hwnd never took the foreground; scene abandoned" >&2
  return 1
}

overall_ok=true
for run in 1 2 3; do
  classic_dpi=fail; classic_dpi_line=""; classic_capture=absent; classic_focus=fail
  packaged_dpi=fail; packaged_dpi_line=""; packaged_capture=absent; packaged_focus=fail
  presentmon_status=fail; presentmon_csv="absent"

  LAUNCH_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-windows-closure-launch.ps1'
  _b_notepad=$(wait_baseline '^BVAGENT SHARE guest->host bv-notepad-started\.log bytes=')
  if send_ok "$LAUNCH_CMD" && wait_after '^BVAGENT SHARE guest->host bv-notepad-started\.log bytes=' "$_b_notepad" 30; then
    hwnd=$(find_hwnd 'Notepad')
    if [[ "$hwnd" =~ ^[0-9]+$ ]]; then
      send "WINBOUNDS $hwnd 50 60 700 500" "^BVAGENT WINBOUNDS $hwnd 50 60 700 500 -> OK WINBOUNDS$" || true
      classic_focus=fail
      if focus_window "$hwnd" "$(hid_point 400 78)"; then classic_focus=pass; fi
      DPI_CMD="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-window-dpi.ps1 -Hwnd $hwnd"
      if send_ok "$DPI_CMD"; then
        classic_dpi_line=$(grep -E "^BVEFFECTIVEDPI hwnd=$hwnd " "$RUN_LOG" | tail -1 | tr -d '\r')
        [[ -n "$classic_dpi_line" ]] && classic_dpi=pass
      fi
      _b_key=$(wait_baseline 'live input accepted: command=Key\(')
      hexlines=$(typing_hex_for_run "$run")
      while IFS= read -r hexline; do
        [[ -n "$hexline" ]] && printf 'KEY text-hex:%s\n' "$hexline" >> "$INPUT"
      done <<<"$hexlines"
      wait_after 'live input accepted: command=Key\(' "$((_b_key + 2))" 30 || true
      sleep 2
      if [[ "$classic_focus" == pass ]] && capture_active_scanout "classic-run${run}"; then classic_capture=present; fi
      # B6's frame-time clause reports nothing against a static scene (zero DWM
      # presents in fifteen seconds on 2026-09-09). What the scene can be asked
      # is how long it takes to show a change; sample that here, five times, so
      # a threshold can later be declared against measured numbers.
      for sample in 1 2 3 4 5; do
        python3 "$REPO/scripts/measure-glyph-present-latency.py" \
          --iosurface "$OUT/display.fb.iosurface" --input-control "$INPUT" \
          --out "$OUT/latency/classic-run${run}-s${sample}" --key-hex 58 >&2 || true
      done
      # Run PresentMon while the window is up, right after the fresh capture.
      PM_CSV="C:\\BridgeVMClosure\\presentmon-classic-run${run}.csv"
      PM_CMD="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-presentmon-capture.ps1 -PresentMonPath C:\\BridgeVMClosure\\$PRESENTMON_NAME -OutputCsvPath $PM_CSV -Seconds 15"
      _b_pmcsv=$(wait_baseline "^BVAGENT SHARE guest->host presentmon-classic-run${run}\.csv bytes=")
      if send_ok "$PM_CMD"; then
        presentmon_status=pass
        wait_after "^BVAGENT SHARE guest->host presentmon-classic-run${run}\.csv bytes=" "$_b_pmcsv" 60 || true
        [[ -f "$OUT/share/presentmon-classic-run${run}.csv" ]] && presentmon_csv="present"
      fi
      send "WINCLOSE $hwnd" "^BVAGENT WINCLOSE $hwnd -> OK WINCLOSE$" || true
      sleep 2
      send_ok "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-windows-closure-discard.ps1 -Hwnd $hwnd" || true
      wait_window_gone "$hwnd" || true
      sleep 2
    fi
  fi

  # Packaged Notepad restores the previous run's document, so a run that did
  # not reset it is not independent of the one before it.
  send_ok "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-modern-notepad-reset.ps1" || true
  MODERN_CMD='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMClosure\bv-b6-modern-notepad-launch.ps1'
  if send_ok "$MODERN_CMD"; then
    mhwnd=""
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      sleep 2
      mhwnd=$(find_hwnd 'Notepad')
      [[ "$mhwnd" =~ ^[0-9]+$ ]] && break
    done
    if [[ "$mhwnd" =~ ^[0-9]+$ ]]; then
      packaged_focus=fail
      if focus_window "$mhwnd"; then packaged_focus=pass; fi
      DPI_CMD2="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-window-dpi.ps1 -Hwnd $mhwnd"
      if send_ok "$DPI_CMD2"; then
        packaged_dpi_line=$(grep -E "^BVEFFECTIVEDPI hwnd=$mhwnd " "$RUN_LOG" | tail -1 | tr -d '\r')
        [[ -n "$packaged_dpi_line" ]] && packaged_dpi=pass
      fi
      # Dismiss the first-run tip so the tab/menu row is unobstructed. This
      # goes on the live-input channel ($INPUT), not the shell command
      # channel ($CTL) -- POINTER/KEY are input-control verbs, not programs.
      _b_ptr=$(wait_baseline 'live input accepted: command=Pointer\(')
      # "Got it" on the packaged first-run tip, measured at (544, 302) px in
      # captures/packaged-run1-observed/observed.ppm on 2026-09-09.
      printf 'POINTER click:%s\n' "$(hid_point 544 302)" >> "$INPUT"
      wait_after 'live input accepted: command=Pointer\(' "$_b_ptr" 15 || true
      sleep 1
      _b_key2=$(wait_baseline 'live input accepted: command=Key\(')
      hexlines2=$(packaged_typing_hex_for_run "$run")
      while IFS= read -r hexline; do
        [[ -n "$hexline" ]] && printf 'KEY text-hex:%s\n' "$hexline" >> "$INPUT"
      done <<<"$hexlines2"
      wait_after 'live input accepted: command=Key\(' "$((_b_key2 + 2))" 30 || true
      sleep 2
      if [[ "$packaged_focus" == pass ]] && capture_active_scanout "packaged-run${run}"; then
        packaged_capture=present
      elif [[ "$packaged_focus" == pass ]]; then
        python3 "$REPO/scripts/observe-active-iosurface.py" \
          --iosurface "$OUT/display.fb.iosurface" \
          --out "$OUT/captures/packaged-run${run}-observed" >&2 || true
      fi
      send "WINCLOSE $mhwnd" "^BVAGENT WINCLOSE $mhwnd -> OK WINCLOSE$" || true
      sleep 2
      send_ok "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-windows-closure-discard.ps1 -Hwnd $mhwnd" || true
      wait_window_gone "$mhwnd" || true
      sleep 2
    fi
  fi

  python3 - "$runs_json" "$run" "$classic_dpi" "$classic_dpi_line" "$classic_capture" \
    "$packaged_dpi" "$packaged_dpi_line" "$packaged_capture" "$presentmon_status" "$presentmon_csv" \
    "$classic_focus" "$packaged_focus" <<'PY'
import json, sys
path, run, cdpi, cline, ccap, pdpi, pline, pcap, pm, pmcsv, cfoc, pfoc = sys.argv[1:13]
data = json.load(open(path))
data.append({
    "run": int(run), "classic_dpi": cdpi, "classic_dpi_line": cline, "classic_capture": ccap,
    "packaged_dpi": pdpi, "packaged_dpi_line": pline, "packaged_capture": pcap,
    "presentmon_status": pm, "presentmon_csv": pmcsv,
    # An absent capture means nothing without the focus it depended on.
    "classic_focus": cfoc, "packaged_focus": pfoc,
})
json.dump(data, open(path, "w"), indent=2)
PY
  [[ "$classic_focus" == pass && "$packaged_focus" == pass \
     && "$classic_dpi" == pass && "$classic_capture" == present \
     && "$packaged_dpi" == pass && "$packaged_capture" == present ]] || overall_ok=false
done

printf 'shutdown /s /f /t 0\n' >> "$CTL"
wait "$LAUNCHER" 2>/dev/null || true; LAUNCHER=''
{
  echo "width=$WIDTH"; echo "height=$HEIGHT"; echo "logpixels=$LOGPIXELS"
  echo "f1_driver_load=$f1"; echo "f2_resize=$f2"; echo "scale_set=$scale_set"
  echo "runs_complete=$overall_ok"
} > "$OUT/summary.txt"
cat "$OUT/summary.txt"
cat "$runs_json"
[[ "$f1" == pass && "$f2" == pass && "$scale_set" == pass && "$overall_ok" == true ]]
