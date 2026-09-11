# The installed-boot runner owns its probe and cleans that child on EXIT.
SNAPSHOT_LAUNCHER=""

snapshot_stop_launcher() {
  [[ -n "$SNAPSHOT_LAUNCHER" ]] || return 0
  kill -CONT "$SNAPSHOT_LAUNCHER" 2>/dev/null || true
  kill -TERM "$SNAPSHOT_LAUNCHER" 2>/dev/null || true
  wait "$SNAPSHOT_LAUNCHER" 2>/dev/null || true
  SNAPSHOT_LAUNCHER=""
}

snapshot_cleanup() {
  local status=$?
  snapshot_stop_launcher
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
  SNAPSHOT_LAUNCHER=""
  (( status == 0 )) || return 1
  tr '\r' '\n' < "$log" | grep -E '^stop: PSCI .*\(system off\)' > /dev/null || return 1
  tr '\r' '\n' < "$log" | grep -E '^NVMe (second namespace )?disk written back:' > /dev/null
}
