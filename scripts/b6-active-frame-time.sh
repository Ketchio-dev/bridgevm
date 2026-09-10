#!/usr/bin/env bash
# Observe active Notepad FrameTime; never a B6 criterion or baseline pass.
b6_collect_active_frame_time() {
  local scene="$1" run="$2" hwnd="$3"
  [[ "$scene" == classic || "$scene" == packaged ]] || return 1
  [[ "$run" =~ ^[123]$ && "$hwnd" =~ ^[1-9][0-9]*$ ]] || return 1
  local name_pattern='^[A-Za-z0-9._ -]+[.]exe$'
  [[ "$PRESENTMON_NAME" =~ $name_pattern ]] || return 1
  local stem="presentmon-$scene-run$run" csv command log_offset
  csv="$OUT/share/$stem.csv"
  [[ ! -e "$csv" && ! -L "$csv" ]] || return 1
  [[ "$(foreground_hwnd)" == "$hwnd" ]] || return 1
  local share_pattern="^BVAGENT SHARE guest->host $stem\.csv bytes="
  local share_before key_before
  share_before=$(wait_baseline "$share_pattern")
  key_before=$(wait_baseline 'live input accepted: command=Key\(')
  log_offset=$(wc -c < "$RUN_LOG") || return 1
  command="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-presentmon-capture.ps1 -PresentMonPath \"C:\\BridgeVMClosure\\$PRESENTMON_NAME\" -OutputCsvPath \"C:\\BridgeVMClosure\\$stem.csv\" -Seconds 15"
  # Only this child uses the agent command channel during collection.
  # The parent drives the separate HID input channel, not a synthetic renderer.
  send_ok "$command" > "$OUT/$stem.command.log" 2>&1 &
  local collector=$! deadline=$((SECONDS + 90)) issued=0 status=0
  while kill -0 "$collector" 2>/dev/null; do
    if ! kill -0 "$LAUNCHER" 2>/dev/null || (( SECONDS >= deadline )); then
      status=1; break
    fi
    if ! printf 'KEY text-hex:58\n' >> "$INPUT"; then status=1; break; fi
    issued=$((issued + 1))
    sleep 0.5 || { status=1; break; }
  done
  if (( status != 0 )); then kill "$collector" 2>/dev/null || true; fi
  wait "$collector" || status=1
  (( status == 0 && issued > 0 )) || return 1
  wait_after 'live input accepted: command=Key\(' "$((key_before + issued - 1))" 30 || return 1
  [[ "$(foreground_hwnd)" == "$hwnd" ]] || return 1
  wait_after "$share_pattern" "$share_before" 60 || return 1
  python3 "$REPO/scripts/b6_presentmon_evidence.py" "$RUN_LOG" "$log_offset" "$csv" "$issued" \
    > "$OUT/$stem.frame-times.json"
}
