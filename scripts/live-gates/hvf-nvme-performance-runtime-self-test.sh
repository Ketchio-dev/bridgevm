#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
source "$REPO/scripts/live-gates/live-process-cleanup.sh"
source "$REPO/scripts/live-gates/hvf-nvme-performance-runtime.sh"
ROOT="${1:?runtime self-test needs a temporary directory}"

mkdir -p "$ROOT/boot"
printf 'READY\r\n' > "$ROOT/boot/run.log"
( BOOT="$ROOT/boot"; VM_PID=""; wait_log '^READY$' 1 1; ! wait_log '^MISSING$' 1 1 )

runtime_case() {
  local expected="$1" initial="$2" receipt="$3" validate="$4" writer="$5" actual
  if (
    RECEIPT_WRITTEN="$receipt"; VALIDATE_ONLY="$validate"; INVALID_REASON=self-test
    VM_PID=""; POWER_MONITOR_PID=""
    terminate_vm() { :; }; stop_power_monitor() { :; }
    write_failed_receipt() { [[ "$writer" == pass ]] && RECEIPT_WRITTEN=1; }
    trap on_exit EXIT
    exit "$initial"
  ); then actual=0; else actual=$?; fi
  [[ "$actual" == "$expected" ]]
}

runtime_case 0 0 1 0 pass
runtime_case 0 0 0 1 pass
runtime_case 1 0 0 0 pass
runtime_case 7 7 0 0 pass
runtime_case 7 7 1 0 pass
runtime_case 1 0 0 0 fail

{
  monitor_ready="$ROOT/monitor-ready"
  python3 -c 'import signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); open(sys.argv[1], "w").close(); time.sleep(60)' "$monitor_ready" &
  monitor=$!
  for _ in {1..100}; do [[ -f "$monitor_ready" ]] && break; sleep 0.01; done
  [[ -f "$monitor_ready" ]]
  bridgevm_terminate_owned_pid_bounded "$monitor" 2 20
  ! kill -0 "$monitor" 2>/dev/null
} 2>/dev/null

(
  set -m
  ready="$ROOT/group-ready"
  python3 -c 'import os,signal,sys,time
pid=os.fork()
if pid:
    while not os.path.exists(sys.argv[1]): time.sleep(.01)
    os._exit(0)
signal.signal(signal.SIGTERM, signal.SIG_IGN)
open(sys.argv[1], "w").close()
time.sleep(60)' "$ready" &
  group=$!
  bridgevm_wait_for_tier_group "$group" "$ROOT/no-cancel" self-test || exit 1
  [[ "$BRIDGEVM_TIER_STATUS" == 125 ]] || exit 1
  ! bridgevm_process_group_alive "$group" || exit 1
)

echo "HVF NVMe performance runtime self-test: PASS"
