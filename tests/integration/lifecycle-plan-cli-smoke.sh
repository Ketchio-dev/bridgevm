#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-lifecycle-plan.XXXXXX")"
VM_LOCAL="lifecycle-plan-local"
VM_SOCKET="lifecycle-plan-socket"
VM_FAST="lifecycle-plan-fast"
DAEMON_PID=""

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

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm start "$VM_LOCAL" >/dev/null

local_missing="$(bridgevm lifecycle-plan "$VM_LOCAL" --action suspend)"
assert_contains "$local_missing" "Lifecycle plan for $VM_LOCAL" "local missing socket"
assert_contains "$local_missing" "Action: suspend" "local missing socket"
assert_contains "$local_missing" "Current state: running" "local missing socket"
assert_contains "$local_missing" "Target state: suspended" "local missing socket"
assert_contains "$local_missing" "Backend: qemu-qmp" "local missing socket"
assert_contains "$local_missing" "Metadata only: true" "local missing socket"
assert_contains "$local_missing" "Executable: false" "local missing socket"
assert_contains "$local_missing" "QMP command: stop" "local missing socket"
assert_contains "$local_missing" "QMP socket available: false" "local missing socket"
assert_contains "$local_missing" "Blocker: qmp-socket-unavailable:" "local missing socket"

local_qmp="$(bridgevm qmp-socket "$VM_LOCAL")"
[[ ! -e "$local_qmp" ]] || fail "local lifecycle-plan created or required a QMP socket marker"
mkdir -p "$(dirname "$local_qmp")"
printf 'presence marker only\n' >"$local_qmp"
local_ready="$(bridgevm lifecycle-plan "$VM_LOCAL" --action suspend)"
assert_contains "$local_ready" "Executable: true" "local ready socket"
assert_contains "$local_ready" "QMP socket available: true" "local ready socket"
assert_contains "$local_ready" "Blockers: none" "local ready socket"

bridgevm create "$VM_FAST" --os ubuntu --arch arm64 --mode fast >/dev/null
fast_blocked="$(bridgevm lifecycle-plan "$VM_FAST" --action suspend)"
assert_contains "$fast_blocked" "Backend: apple-vz" "fast blocked"
assert_contains "$fast_blocked" "Executable: false" "fast blocked"
assert_contains "$fast_blocked" "Blocker: invalid-lifecycle-transition:" "fast blocked"
assert_contains "$fast_blocked" "Fast Mode suspend/resume is wired through the runner via Apple VZ" "fast blocked"

fast_resume_blocked="$(bridgevm lifecycle-plan "$VM_FAST" --action resume)"
assert_contains "$fast_resume_blocked" "Action: resume" "fast resume blocked"
assert_contains "$fast_resume_blocked" "Target state: running" "fast resume blocked"
assert_contains "$fast_resume_blocked" "Executable: false" "fast resume blocked"
assert_contains "$fast_resume_blocked" "Blocker: invalid-lifecycle-transition:stopped->running" "fast resume blocked"

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
socket_state="$STORE/vms/$VM_SOCKET.vmbridge/metadata/state.json"
printf '{"state":"suspended","updated_at_unix":1}\n' >"$socket_state"

socket_missing="$(bridgevm_socket lifecycle-plan "$VM_SOCKET" --action resume)"
assert_contains "$socket_missing" "Lifecycle plan for $VM_SOCKET" "socket missing"
assert_contains "$socket_missing" "Action: resume" "socket missing"
assert_contains "$socket_missing" "Current state: suspended" "socket missing"
assert_contains "$socket_missing" "Target state: running" "socket missing"
assert_contains "$socket_missing" "QMP command: cont" "socket missing"
assert_contains "$socket_missing" "Executable: false" "socket missing"

socket_qmp="$(bridgevm_socket qmp-socket "$VM_SOCKET")"
[[ ! -e "$socket_qmp" ]] || fail "socket lifecycle-plan created or required a QMP socket marker"
mkdir -p "$(dirname "$socket_qmp")"
printf 'presence marker only\n' >"$socket_qmp"
socket_ready="$(bridgevm_socket lifecycle-plan "$VM_SOCKET" --action resume)"
assert_contains "$socket_ready" "Executable: true" "socket ready"
assert_contains "$socket_ready" "QMP socket available: true" "socket ready"
assert_contains "$socket_ready" "Blockers: none" "socket ready"

echo "PASS: lifecycle plan CLI/socket metadata smoke ($STORE)"
