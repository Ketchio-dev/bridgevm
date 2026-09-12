# Each launch gets its own process group, including the runner's probe.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/live-gates/live-process-cleanup.sh"
set -m
SNAPSHOT_LAUNCHER=""
snapshot_stop_launcher() {
  [[ -n "$SNAPSHOT_LAUNCHER" ]] || return 0
  kill -CONT -- "-$SNAPSHOT_LAUNCHER" 2>/dev/null || true
  bridgevm_terminate_process_group_bounded "$SNAPSHOT_LAUNCHER" || return 1
  wait "$SNAPSHOT_LAUNCHER" 2>/dev/null || true
  SNAPSHOT_LAUNCHER=""
}

snapshot_cleanup() {
  local status=$?
  snapshot_stop_launcher || return 1
  rm -rf "$WORK"
  return "$status"
}
snapshot_shutdown() {
  local ctl=$1 log=$2 deadline=$((SECONDS + STEP_TIMEOUT)) status=0
  printf 'POWEROFF\n' >> "$ctl" || return 1
  while kill -0 "$SNAPSHOT_LAUNCHER" 2>/dev/null; do
    (( SECONDS < deadline )) || { snapshot_stop_launcher; return 1; }
    sleep 0.25
  done
  wait "$SNAPSHOT_LAUNCHER" || status=$?
  if bridgevm_process_group_alive "$SNAPSHOT_LAUNCHER"; then snapshot_stop_launcher; return 1; fi
  SNAPSHOT_LAUNCHER=""
  (( status == 0 )) && tr '\r' '\n' < "$log" | grep -E '^stop: PSCI .*\(system off\)' > /dev/null || return 1
  tr '\r' '\n' < "$log" | grep -E '^NVMe (second namespace )?disk written back:' > /dev/null
}
