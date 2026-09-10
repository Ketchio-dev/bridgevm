# Read-only apartment comparison, after the original failure frame is saved.
b6_failure_diagnostics() {
  [[ "$2" == packaged && "$3" == tip-dismissal-failed ]] || return 0
  local hwnd="${mhwnd:-}" mode command before completed STEP_TIMEOUT=20
  [[ "$hwnd" =~ ^[1-9][0-9]*$ && "${WIDTH:-}" =~ ^[1-9][0-9]*$ && "${HEIGHT:-}" =~ ^[1-9][0-9]*$ ]] || return 0
  for mode in Sta Mta; do
    command="powershell -$mode -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-uia-diagnostic.ps1 -Hwnd $hwnd -Width $WIDTH -Height $HEIGHT"
    before=$(wc -l < "$RUN_LOG") || return 0
    echo "UIA-DIAGNOSTIC: mode=$mode hwnd=$hwnd observation-only=true" >&2
    send "$command" '^BVAGENT END ' || return 0
    completed=$(tail -n "+$((before + 1))" "$RUN_LOG" | tr '\r' '\n') || return 0
    if ! grep -Fx "BVAGENT END $command" <<<"$completed" >/dev/null; then
      echo "UIA-DIAGNOSTIC: matching completion absent; no further query" >&2
      return 0
    fi
  done
  return 0
}
