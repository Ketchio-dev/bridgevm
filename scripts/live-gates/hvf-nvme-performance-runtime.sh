#!/usr/bin/env bash
# Runtime helpers sourced by the sealed T16 physical-Mac runner.

wait_log() {
  local pattern count timeout deadline observed
  pattern="$1"; count="$2"; timeout="$3"; deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    observed="$(tr '\r' '\n' < "$BOOT/run.log" 2>/dev/null | grep -Ec "$pattern" || true)"
    [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= count )) && return 0
    [[ -z "${VM_PID:-}" ]] || kill -0 "$VM_PID" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}

terminate_vm() {
  if [[ "${VM_PID:-}" =~ ^[1-9][0-9]*$ ]] && kill -0 "$VM_PID" 2>/dev/null; then
    pkill -TERM -P "$VM_PID" 2>/dev/null || true
    kill -TERM "$VM_PID" 2>/dev/null || true
    for _ in {1..10}; do kill -0 "$VM_PID" 2>/dev/null || break; sleep 1; done
    if kill -0 "$VM_PID" 2>/dev/null; then
      pkill -KILL -P "$VM_PID" 2>/dev/null || true
      kill -KILL "$VM_PID" 2>/dev/null || true
    fi
    wait "$VM_PID" 2>/dev/null || true
  fi
  VM_PID=""
}

stop_power_monitor() {
  if [[ -n "$POWER_MONITOR_PID" ]] && kill -0 "$POWER_MONITOR_PID" 2>/dev/null; then kill "$POWER_MONITOR_PID" 2>/dev/null || true; wait "$POWER_MONITOR_PID" 2>/dev/null || true; fi
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
  local status="$?"
  trap - EXIT
  [[ "$RECEIPT_WRITTEN" == 1 || "$VALIDATE_ONLY" == 1 || "$status" != 0 ]] || status=1
  terminate_vm; stop_power_monitor
  [[ "$RECEIPT_WRITTEN" == 1 || "$VALIDATE_ONLY" == 1 ]] || write_failed_receipt "$INVALID_REASON" || true
  exit "$status"
}

nvme_perf_runtime_self_test() {
  local root="$1"
  mkdir -p "$root/boot"; printf 'READY\r\n' > "$root/boot/run.log"
  ( BOOT="$root/boot"; VM_PID=""; wait_log '^READY$' 1 1; ! wait_log '^MISSING$' 1 1 )
  ! ( RECEIPT_WRITTEN=0; VALIDATE_ONLY=0; INVALID_REASON=self-test; VM_PID=""; POWER_MONITOR_PID="";
      terminate_vm() { :; }; stop_power_monitor() { :; }; write_failed_receipt() { RECEIPT_WRITTEN=1; };
      trap on_exit EXIT; true )
}
