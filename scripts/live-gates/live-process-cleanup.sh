#!/usr/bin/env bash
# Bounded process cleanup shared by the local physical-Mac queue and live tiers.

bridgevm_canonical_pid() {
  [[ "${1:-}" =~ ^[1-9][0-9]*$ && "$1" -gt 1 ]]
}

bridgevm_process_alive() {
  local pid="$1" state
  kill -0 "$pid" 2>/dev/null || return 1
  state="$(ps -o state= -p "$pid" 2>/dev/null | tr -d ' ')"
  [[ "$state" != Z* ]]
}

bridgevm_process_group_alive() {
  bridgevm_canonical_pid "$1" && kill -0 -- "-$1" 2>/dev/null
}

bridgevm_wait_pid_gone() {
  local pid="$1" attempts="$2" i
  for ((i = 0; i < attempts; i++)); do
    bridgevm_process_alive "$pid" || return 0
    sleep 0.1
  done
  ! bridgevm_process_alive "$pid"
}

bridgevm_wait_group_gone() {
  local pgid="$1" attempts="$2" i
  for ((i = 0; i < attempts; i++)); do
    bridgevm_process_group_alive "$pgid" || return 0
    sleep 0.1
  done
  ! bridgevm_process_group_alive "$pgid"
}

bridgevm_terminate_owned_pid_bounded() {
  local pid="$1" term_attempts="${2:-20}" kill_attempts="${3:-50}"
  bridgevm_canonical_pid "$pid" || return 2
  if ! bridgevm_process_alive "$pid"; then wait "$pid" 2>/dev/null || true; return 0; fi
  kill -TERM "$pid" 2>/dev/null || true
  if bridgevm_wait_pid_gone "$pid" "$term_attempts"; then wait "$pid" 2>/dev/null || true; return 0; fi
  kill -KILL "$pid" 2>/dev/null || true
  bridgevm_wait_pid_gone "$pid" "$kill_attempts" || return 1
  wait "$pid" 2>/dev/null || true
}

bridgevm_terminate_process_group_bounded() {
  local pgid="$1" term_attempts="${2:-50}" kill_attempts="${3:-50}" own_pgid
  bridgevm_canonical_pid "$pgid" || return 2
  own_pgid="$(ps -o pgid= -p "$$" 2>/dev/null | tr -d ' ')"
  [[ "$pgid" != "$own_pgid" ]] || return 2
  bridgevm_process_group_alive "$pgid" || return 0
  kill -TERM -- "-$pgid" 2>/dev/null || true
  bridgevm_wait_group_gone "$pgid" "$term_attempts" && return 0
  kill -KILL -- "-$pgid" 2>/dev/null || true
  bridgevm_wait_group_gone "$pgid" "$kill_attempts"
}

bridgevm_cleanup_log() {
  declare -F log >/dev/null && log "$*"
  return 0
}

# Sets BRIDGEVM_TIER_STATUS. A nonzero function return means group cleanup
# could not be confirmed and the caller must fence the queue.
bridgevm_wait_for_tier_group() {
  local tier_pid="$1" cancel_path="$2" job_id="$3" observed_pgid own_pgid status=0 residue=0
  BRIDGEVM_TIER_STATUS=0
  bridgevm_canonical_pid "$tier_pid" || return 2
  own_pgid="$(ps -o pgid= -p "$$" 2>/dev/null | tr -d ' ')"
  observed_pgid="$(ps -o pgid= -p "$tier_pid" 2>/dev/null | tr -d ' ')"
  if [[ "$tier_pid" == "$own_pgid" || ( -n "$observed_pgid" && "$observed_pgid" != "$tier_pid" ) ]]; then
    bridgevm_cleanup_log "job $job_id lacks an isolated process group"
    bridgevm_terminate_owned_pid_bounded "$tier_pid" || true
    BRIDGEVM_TIER_STATUS=126
    return 1
  fi
  while bridgevm_process_alive "$tier_pid"; do
    if [[ -f "$cancel_path" ]]; then
      bridgevm_cleanup_log "job $job_id canceled; stopping its process group"
      bridgevm_terminate_process_group_bounded "$tier_pid" || return 1
      break
    fi
    sleep 2
  done
  if bridgevm_process_alive "$tier_pid"; then BRIDGEVM_TIER_STATUS=126; return 1; fi
  wait "$tier_pid" 2>/dev/null || status=$?
  if bridgevm_process_group_alive "$tier_pid"; then
    residue=1
    bridgevm_cleanup_log "job $job_id left a live process in tier group $tier_pid"
    bridgevm_terminate_process_group_bounded "$tier_pid" || { BRIDGEVM_TIER_STATUS=126; return 1; }
  fi
  [[ "$residue" == 0 || "$status" != 0 ]] || status=125
  BRIDGEVM_TIER_STATUS="$status"
}
