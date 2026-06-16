#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-qmp-control.XXXXXX")"
VM_LOCAL="qmp-control-local"
VM_SOCKET="qmp-control-socket"
DAEMON_PID=""
FAKE_QMP_PID=""

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fullvm_runner() {
  cargo run --quiet -p fullvm-runner -- "$1" --store "$STORE" "${@:2}"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  if [[ -n "${FAKE_QMP_LOG:-}" && -f "$FAKE_QMP_LOG" ]]; then
    echo "Fake QMP log: $FAKE_QMP_LOG" >&2
    cat "$FAKE_QMP_LOG" >&2
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

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded; got: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

stop_background_processes() {
  if [[ -n "${FAKE_QMP_PID:-}" ]]; then
    kill "$FAKE_QMP_PID" 2>/dev/null || true
    wait "$FAKE_QMP_PID" 2>/dev/null || true
  fi
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

start_fake_qmp() {
  local socket_path="$1"
  local expected_command="$2"

  rm -f "$socket_path"
  FAKE_QMP_LOG="$STORE/fake-qmp-$expected_command.log"
  python3 - "$socket_path" "$expected_command" >"$FAKE_QMP_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys
import time

socket_path = sys.argv[1]
expected_command = sys.argv[2]
os.makedirs(os.path.dirname(socket_path), exist_ok=True)
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(1)
server.settimeout(10)

conn, _ = server.accept()
with conn:
    stream = conn.makefile("rwb")
    stream.write(b'{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}\n')
    stream.flush()

    capabilities = stream.readline().decode("utf-8")
    if "qmp_capabilities" not in capabilities:
        raise SystemExit(f"missing capabilities command: {capabilities!r}")
    stream.write(b'{"return":{}}\n')
    stream.flush()

    command = stream.readline().decode("utf-8")
    payload = json.loads(command)
    if payload.get("execute") != expected_command:
        raise SystemExit(f"expected {expected_command!r}, got {command!r}")
    if expected_command == "query-status":
        stream.write(b'{"event":"STOP","timestamp":{"seconds":1710000100,"microseconds":0}}\n')
        stream.write(b'{"return":{"status":"running","running":true}}\n')
    else:
        stream.write(b'{"return":{}}\n')
    stream.flush()

print(f"fake QMP observed {expected_command}")
time.sleep(0.05)
PY
  FAKE_QMP_PID=$!

  for _ in {1..100}; do
    if [[ -S "$socket_path" ]]; then
      return
    fi
    sleep 0.05
  done
  fail "fake QMP socket was not ready: $socket_path"
}

wait_fake_qmp() {
  wait "$FAKE_QMP_PID" || fail "fake QMP failed"
  FAKE_QMP_PID=""
}

leave_stale_qmp_socket() {
  local socket_path="$1"

  rm -f "$socket_path"
  python3 - "$socket_path" <<'PY'
import os
import socket
import sys

socket_path = sys.argv[1]
os.makedirs(os.path.dirname(socket_path), exist_ok=True)
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.close()
PY
  [[ -S "$socket_path" ]] || fail "stale QMP socket was not created: $socket_path"
}

start_closing_qmp() {
  local socket_path="$1"

  rm -f "$socket_path"
  FAKE_QMP_LOG="$STORE/fake-qmp-close.log"
  python3 - "$socket_path" >"$FAKE_QMP_LOG" 2>&1 <<'PY' &
import os
import socket
import sys

socket_path = sys.argv[1]
os.makedirs(os.path.dirname(socket_path), exist_ok=True)
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(1)
server.settimeout(10)

conn, _ = server.accept()
conn.close()
server.close()
print("fake QMP closed before greeting")
PY
  FAKE_QMP_PID=$!

  for _ in {1..100}; do
    if [[ -S "$socket_path" ]]; then
      return
    fi
    sleep 0.05
  done
  fail "closing fake QMP socket was not ready: $socket_path"
}

trap stop_background_processes EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
local_qmp="$(bridgevm qmp-socket "$VM_LOCAL")"

start_fake_qmp "$local_qmp" stop
local_stop="$(bridgevm qmp-stop "$VM_LOCAL")"
wait_fake_qmp
assert_contains "$local_stop" "QMP command sent: stop" "local qmp-stop"
assert_contains "$local_stop" "VM: $VM_LOCAL" "local qmp-stop"

start_fake_qmp "$local_qmp" cont
local_cont="$(bridgevm qmp-cont "$VM_LOCAL")"
wait_fake_qmp
assert_contains "$local_cont" "QMP command sent: cont" "local qmp-cont"
assert_contains "$local_cont" "VM: $VM_LOCAL" "local qmp-cont"

start_fake_qmp "$local_qmp" query-status
local_status="$(bridgevm qmp-status "$VM_LOCAL")"
wait_fake_qmp
assert_contains "$local_status" "QMP status: running" "local qmp-status"
assert_contains "$local_status" "Running: true" "local qmp-status"

start_fake_qmp "$local_qmp" query-status
runner_status="$(fullvm_runner "$VM_LOCAL" --qmp-status)"
wait_fake_qmp
assert_contains "$runner_status" "qmp_status: running" "fullvm-runner qmp-status"
assert_contains "$runner_status" "running: true" "fullvm-runner qmp-status"

rm -f "$local_qmp"
local_missing_status="$(bridgevm qmp-status "$VM_LOCAL")"
assert_contains \
  "$local_missing_status" \
  "QMP socket unavailable: $local_qmp" \
  "local missing qmp-status socket"
runner_missing_status="$(fullvm_runner "$VM_LOCAL" --qmp-status)"
assert_contains \
  "$runner_missing_status" \
  "QMP socket unavailable: $local_qmp" \
  "fullvm-runner missing qmp-status socket"
assert_fails_contains \
  "local missing qmp socket" \
  "QMP socket unavailable: $local_qmp" \
  bridgevm qmp-stop "$VM_LOCAL"

leave_stale_qmp_socket "$local_qmp"
local_stale_status="$(bridgevm qmp-status "$VM_LOCAL")"
assert_contains \
  "$local_stale_status" \
  "QMP socket unavailable: $local_qmp" \
  "local stale qmp-status socket"
runner_stale_status="$(fullvm_runner "$VM_LOCAL" --qmp-status)"
assert_contains \
  "$runner_stale_status" \
  "QMP socket unavailable: $local_qmp" \
  "fullvm-runner stale qmp-status socket"
assert_fails_contains \
  "local stale qmp socket" \
  "failed to send QMP stop" \
  bridgevm qmp-stop "$VM_LOCAL"

start_closing_qmp "$local_qmp"
local_closed_status="$(bridgevm qmp-status "$VM_LOCAL")"
wait_fake_qmp
assert_contains \
  "$local_closed_status" \
  "QMP socket unavailable: $local_qmp" \
  "local closing qmp-status socket"

start_closing_qmp "$local_qmp"
runner_closed_status="$(fullvm_runner "$VM_LOCAL" --qmp-status)"
wait_fake_qmp
assert_contains \
  "$runner_closed_status" \
  "QMP socket unavailable: $local_qmp" \
  "fullvm-runner closing qmp-status socket"

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

bridgevm create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
socket_qmp="$(bridgevm_socket qmp-socket "$VM_SOCKET")"

start_fake_qmp "$socket_qmp" cont
socket_cont="$(bridgevm_socket qmp-cont "$VM_SOCKET")"
wait_fake_qmp
assert_contains "$socket_cont" "QMP command sent: cont" "socket qmp-cont"
assert_contains "$socket_cont" "VM: $VM_SOCKET" "socket qmp-cont"

start_fake_qmp "$socket_qmp" stop
socket_stop="$(bridgevm_socket qmp-stop "$VM_SOCKET")"
wait_fake_qmp
assert_contains "$socket_stop" "QMP command sent: stop" "socket qmp-stop"
assert_contains "$socket_stop" "VM: $VM_SOCKET" "socket qmp-stop"

start_fake_qmp "$socket_qmp" query-status
socket_status="$(bridgevm_socket qmp-status "$VM_SOCKET")"
wait_fake_qmp
assert_contains "$socket_status" "QMP status: running" "socket qmp-status"
assert_contains "$socket_status" "Running: true" "socket qmp-status"

rm -f "$socket_qmp"
socket_missing_status="$(bridgevm_socket qmp-status "$VM_SOCKET")"
assert_contains \
  "$socket_missing_status" \
  "QMP socket unavailable: $socket_qmp" \
  "socket missing qmp-status socket"
assert_fails_contains \
  "socket missing qmp socket" \
  "QMP socket unavailable: $socket_qmp" \
  bridgevm_socket qmp-cont "$VM_SOCKET"

leave_stale_qmp_socket "$socket_qmp"
socket_stale_status="$(bridgevm_socket qmp-status "$VM_SOCKET")"
assert_contains \
  "$socket_stale_status" \
  "QMP socket unavailable: $socket_qmp" \
  "socket stale qmp-status socket"
assert_fails_contains \
  "socket stale qmp socket" \
  "QMP I/O error" \
  bridgevm_socket qmp-cont "$VM_SOCKET"

start_closing_qmp "$socket_qmp"
socket_closed_status="$(bridgevm_socket qmp-status "$VM_SOCKET")"
wait_fake_qmp
assert_contains \
  "$socket_closed_status" \
  "QMP socket unavailable: $socket_qmp" \
  "socket closing qmp-status socket"

echo "PASS: QMP control CLI/socket smoke ($STORE)"
