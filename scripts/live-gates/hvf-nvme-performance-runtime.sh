#!/usr/bin/env bash
# Runtime helpers sourced by the sealed T16 physical-Mac runner.
source "$REPO/scripts/live-gates/live-process-cleanup.sh"

wait_log() {
  local pattern count timeout deadline observed
  pattern="$1"; count="$2"; timeout="$3"; deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if [[ -f "$BOOT/run.log" ]]; then
      observed="$(tr '\r' '\n' < "$BOOT/run.log" | grep -Ec "$pattern" || true)"
    else observed=0; fi
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    [[ -z "${VM_PID:-}" ]] || kill -0 "$VM_PID" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}

terminate_vm() {
  if [[ "${VM_PID:-}" =~ ^[1-9][0-9]*$ ]] && kill -0 "$VM_PID" 2>/dev/null; then
    pkill -TERM -P "$VM_PID" 2>/dev/null || true
    bridgevm_terminate_owned_pid_bounded "$VM_PID" 100 50 || return 1
  fi
  VM_PID=""
}

stop_power_monitor() {
  if [[ -n "$POWER_MONITOR_PID" ]]; then
    bridgevm_terminate_owned_pid_bounded "$POWER_MONITOR_PID" 20 50 || return 1
  fi
  POWER_MONITOR_PID=""; NVME_PERF_POWER_LOG_HASH="$(seal "$POWER_LOG")"; export NVME_PERF_POWER_LOG_HASH
}

write_failed_receipt() {
  local reason="$1" public_reason
  case "$reason" in
    workload-result-timeout) public_reason=workload-timeout ;;
    agent-service-timeout) public_reason=guest-unreachable ;;
    power-source-changed-or-unknown|power-monitor-*) public_reason=power-source-invalid ;;
    workload-*) public_reason=workload-failed ;;
    boot-wrapper-*|vm-exited-before-result) public_reason=worker-interrupted ;;
    *) public_reason=artifact-invalid ;;
  esac
  NVME_PERF_POWER_SOURCE_END="$(power_source)"; [[ -n "$NVME_PERF_POWER_SOURCE_END" ]] || NVME_PERF_POWER_SOURCE_END=unknown
  stop_power_monitor; export NVME_PERF_POWER_SOURCE_END
  python3 "$WRITER" --failed-reason "$public_reason" --output "$OUT/receipt.json" || return 1
  RECEIPT_WRITTEN=1
}

on_exit() {
  local status="$?" cleanup_status=0
  trap - EXIT
  [[ "$RECEIPT_WRITTEN" == 1 || "$VALIDATE_ONLY" == 1 || "$status" != 0 ]] || status=1
  terminate_vm || cleanup_status=1
  if [[ "$RECEIPT_WRITTEN" == 1 || "$VALIDATE_ONLY" == 1 ]]; then
    stop_power_monitor || cleanup_status=1
  else
    write_failed_receipt "$INVALID_REASON" || cleanup_status=1
  fi
  [[ "$cleanup_status" == 0 ]] || status=1
  exit "$status"
}
