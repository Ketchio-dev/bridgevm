# Shared helpers for driving the guest agent's control channel.
#
# These four were byte-identical in b6-cell-capture.sh, b6-cell-set-scale.sh and
# windows-1.0-closure-interact.sh. Six further copies of send()/wait_for() exist
# across the verify-*.sh gates and have all drifted apart from each other; those
# are deliberately left alone, because whether their differences are intentional
# cannot be read off the files and re-proving each one means reopening a sealed
# live gate. This library is the three that had not yet diverged.
#
# The caller owns the state: RUN_LOG (the launcher's log), CTL (the agent
# control file), LAUNCHER (the launcher's pid), STEP_TIMEOUT and AGENT_TIMEOUT.
# The parameter expansions below are the contract, and fail loudly rather than
# letting a caller that forgot one grep an empty path forever.

wait_for() {
  local pattern="$1" count="$2" timeout="$3" observed
  local log="${RUN_LOG:?agent-channel-lib needs RUN_LOG}"
  local pid="${LAUNCHER:?agent-channel-lib needs LAUNCHER}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    observed=$(grep -cE "$pattern" "$log" 2>/dev/null || true)
    (( observed >= count )) && return 0
    kill -0 "$pid" 2>/dev/null || return 1
    sleep 0.25
  done
  return 1
}

send() {
  local command="$1" pattern="${2:-^BVAGENT END }" before
  local log="${RUN_LOG:?agent-channel-lib needs RUN_LOG}"
  local ctl="${CTL:?agent-channel-lib needs CTL}"
  before=$(grep -cE "$pattern" "$log" 2>/dev/null || true)
  printf '%s\n' "$command" >> "$ctl"
  wait_for "$pattern" $((before + 1)) "${STEP_TIMEOUT:?agent-channel-lib needs STEP_TIMEOUT}" || {
    echo "FAIL: no reply for ${command:0:100}" >&2
    return 1
  }
}

send_ok() {
  local command="$1" before line
  local log="${RUN_LOG:?agent-channel-lib needs RUN_LOG}"
  before=$(grep -cE '^BVAGENT CMD .* exit=' "$log" 2>/dev/null || true)
  send "$command" '^BVAGENT END ' || return 1
  line=$(grep -E '^BVAGENT CMD .* exit=' "$log" | tail -1)
  [[ $(grep -cE '^BVAGENT CMD .* exit=' "$log") -gt $before && "$line" == *' exit=0' ]]
}

wait_firstboot() {
  local log="${RUN_LOG:?agent-channel-lib needs RUN_LOG}"
  local deadline=$((SECONDS + ${AGENT_TIMEOUT:?agent-channel-lib needs AGENT_TIMEOUT}))
  local command='powershell -NoProfile -Command "& schtasks.exe /Query /TN BridgeVM-VioGpu3DFirstBoot *> $null; $task=($LASTEXITCODE -eq 0); $ready=(Test-Path C:\BridgeVM\stage3.flag) -and (-not $task); if($ready){Write-Output BVFIRSTBOOT_READY; exit 0}; Write-Output BVFIRSTBOOT_PENDING; exit 3"'
  while (( SECONDS < deadline )); do
    send_ok "$command" && grep -Eq '^BVFIRSTBOOT_READY\r?$' "$log" && return 0
    sleep 5
  done
  return 1
}
