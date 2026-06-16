#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-log-viewer.XXXXXX")"
VM_NAME="legacy-logs"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly contained '$needle'; got: $haystack" ;;
    *) ;;
  esac
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
mkdir -p "$BUNDLE/logs"
printf 'qemu-line-1\nqemu-line-2\nqemu-line-3\n' >"$BUNDLE/logs/qemu.log"
printf 'serial-line-1\nserial-line-2\nserial-line-3\n' >"$BUNDLE/logs/serial.log"

local_output="$(bridgevm logs qemu "$VM_NAME" --bytes 12)"
assert_contains "$local_output" "Log for $VM_NAME" "local qemu log"
assert_contains "$local_output" "Kind: Qemu" "local qemu log"
assert_contains "$local_output" "Path: $BUNDLE/logs/qemu.log" "local qemu log"
assert_contains "$local_output" "Exists: true" "local qemu log"
assert_contains "$local_output" "Bytes: 36" "local qemu log"
assert_contains "$local_output" "Returned bytes: 12" "local qemu log"
assert_contains "$local_output" "Truncated: true" "local qemu log"
assert_contains "$local_output" "--- log tail ---" "local qemu log"
assert_contains "$local_output" "qemu-line-3" "local qemu log"
assert_not_contains "$local_output" "qemu-line-1" "local qemu bounded tail"
assert_not_contains "$local_output" "qemu-line-2" "local qemu bounded tail"

local_serial_output="$(bridgevm logs serial "$VM_NAME" --bytes 15)"
assert_contains "$local_serial_output" "Log for $VM_NAME" "local serial log"
assert_contains "$local_serial_output" "Kind: Serial" "local serial log"
assert_contains "$local_serial_output" "Path: $BUNDLE/logs/serial.log" "local serial log"
assert_contains "$local_serial_output" "Exists: true" "local serial log"
assert_contains "$local_serial_output" "Bytes: 42" "local serial log"
assert_contains "$local_serial_output" "Returned bytes: 15" "local serial log"
assert_contains "$local_serial_output" "Truncated: true" "local serial log"
assert_contains "$local_serial_output" "--- log tail ---" "local serial log"
assert_contains "$local_serial_output" "serial-line-3" "local serial log"
assert_not_contains "$local_serial_output" "serial-line-1" "local serial bounded tail"
assert_not_contains "$local_serial_output" "serial-line-2" "local serial bounded tail"

NO_LOG_VM="missing-log-file"
NO_LOG_BUNDLE="$STORE/vms/$NO_LOG_VM.vmbridge"
bridgevm create "$NO_LOG_VM" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
missing_log_output="$(bridgevm logs serial "$NO_LOG_VM" --bytes 8)"
assert_contains "$missing_log_output" "Log for $NO_LOG_VM" "missing serial log file"
assert_contains "$missing_log_output" "Kind: Serial" "missing serial log file"
assert_contains "$missing_log_output" "Path: $NO_LOG_BUNDLE/logs/serial.log" "missing serial log file"
assert_contains "$missing_log_output" "Exists: false" "missing serial log file"
assert_contains "$missing_log_output" "Bytes: 0" "missing serial log file"
assert_contains "$missing_log_output" "Returned bytes: 0" "missing serial log file"
assert_contains "$missing_log_output" "Truncated: false" "missing serial log file"
assert_not_contains "$missing_log_output" "--- log tail ---" "missing serial log file"

set +e
missing_output="$(bridgevm logs serial missing-vm 2>&1)"
missing_status=$?
set -e
[[ "$missing_status" -ne 0 ]] || fail "missing vm log unexpectedly succeeded"
assert_contains "$missing_output" "VM not found" "missing vm log"

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

socket_qemu_output="$(bridgevm_socket logs qemu "$VM_NAME" --bytes 12)"
assert_contains "$socket_qemu_output" "Log for $VM_NAME" "socket qemu log"
assert_contains "$socket_qemu_output" "Kind: Qemu" "socket qemu log"
assert_contains "$socket_qemu_output" "Path: $BUNDLE/logs/qemu.log" "socket qemu log"
assert_contains "$socket_qemu_output" "Exists: true" "socket qemu log"
assert_contains "$socket_qemu_output" "Bytes: 36" "socket qemu log"
assert_contains "$socket_qemu_output" "Returned bytes: 12" "socket qemu log"
assert_contains "$socket_qemu_output" "Truncated: true" "socket qemu log"
assert_contains "$socket_qemu_output" "--- log tail ---" "socket qemu log"
assert_contains "$socket_qemu_output" "qemu-line-3" "socket qemu log"
assert_not_contains "$socket_qemu_output" "qemu-line-1" "socket qemu bounded tail"
assert_not_contains "$socket_qemu_output" "qemu-line-2" "socket qemu bounded tail"

socket_output="$(bridgevm_socket logs serial "$VM_NAME" --bytes 14)"
assert_contains "$socket_output" "Log for $VM_NAME" "socket serial log"
assert_contains "$socket_output" "Kind: Serial" "socket serial log"
assert_contains "$socket_output" "Path: $BUNDLE/logs/serial.log" "socket serial log"
assert_contains "$socket_output" "Exists: true" "socket serial log"
assert_contains "$socket_output" "Bytes: 42" "socket serial log"
assert_contains "$socket_output" "Returned bytes: 14" "socket serial log"
assert_contains "$socket_output" "Truncated: true" "socket serial log"
assert_contains "$socket_output" "--- log tail ---" "socket serial log"
assert_contains "$socket_output" "serial-line-3" "socket serial log"
assert_not_contains "$socket_output" "serial-line-1" "socket serial bounded tail"
assert_not_contains "$socket_output" "serial-line-2" "socket serial bounded tail"

socket_missing_log_output="$(bridgevm_socket logs serial "$NO_LOG_VM" --bytes 8)"
assert_contains "$socket_missing_log_output" "Log for $NO_LOG_VM" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Kind: Serial" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Path: $NO_LOG_BUNDLE/logs/serial.log" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Exists: false" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Bytes: 0" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Returned bytes: 0" "socket missing serial log file"
assert_contains "$socket_missing_log_output" "Truncated: false" "socket missing serial log file"
assert_not_contains "$socket_missing_log_output" "--- log tail ---" "socket missing serial log file"

set +e
socket_missing_output="$(bridgevm_socket logs serial missing-vm 2>&1)"
socket_missing_status=$?
set -e
[[ "$socket_missing_status" -ne 0 ]] || fail "socket missing vm log unexpectedly succeeded"
assert_contains "$socket_missing_output" "VM not found" "socket missing vm log"

echo "PASS: log viewer CLI/socket integration smoke ($STORE)"
