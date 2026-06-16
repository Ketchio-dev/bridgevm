#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-suspend-resume.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="suspend-resume-linux"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

for backend in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in lifecycle suspend/resume smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"

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

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend launch attempted: $(cat "$BACKEND_LOG")"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode fast >/dev/null

stopped_suspend="$(bridgevm suspend "$VM_NAME" 2>&1 || true)"
assert_contains "$stopped_suspend" "invalid VM state transition from Stopped to Suspended" \
  "local stopped suspend rejection"

bridgevm start "$VM_NAME" >/dev/null

local_suspend="$(bridgevm suspend "$VM_NAME")"
assert_contains "$local_suspend" "Metadata state recorded for $VM_NAME (suspended)" "local suspend"

local_suspended_status="$(bridgevm status "$VM_NAME")"
assert_contains "$local_suspended_status" "State: suspended" "local suspended status"

local_resume="$(bridgevm resume "$VM_NAME")"
assert_contains "$local_resume" "Metadata state recorded for $VM_NAME (running)" "local resume"

local_running_status="$(bridgevm status "$VM_NAME")"
assert_contains "$local_running_status" "State: running" "local running status"

bridgevm suspend "$VM_NAME" >/dev/null
local_stop_from_suspend="$(bridgevm stop "$VM_NAME")"
assert_contains "$local_stop_from_suspend" "Stopped $VM_NAME" "local stop suspended"

assert_no_backend_launch

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

socket_stopped_suspend="$(bridgevm_socket suspend "$VM_NAME" 2>&1 || true)"
assert_contains "$socket_stopped_suspend" "invalid VM state transition from Stopped to Suspended" \
  "socket stopped suspend rejection"

socket_start="$(bridgevm_socket start "$VM_NAME")"
assert_contains "$socket_start" "Metadata state recorded for $VM_NAME (running)" "socket start"

socket_suspend="$(bridgevm_socket suspend "$VM_NAME")"
assert_contains "$socket_suspend" "Metadata state recorded for $VM_NAME (suspended)" "socket suspend"

socket_suspended_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_suspended_status" "$VM_NAME" "socket suspended status"
assert_contains "$socket_suspended_status" "suspended" "socket suspended status"

socket_resume="$(bridgevm_socket resume "$VM_NAME")"
assert_contains "$socket_resume" "Metadata state recorded for $VM_NAME (running)" "socket resume"

socket_running_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_running_status" "$VM_NAME" "socket running status"
assert_contains "$socket_running_status" "running" "socket running status"

bridgevm_socket stop "$VM_NAME" >/dev/null
assert_no_backend_launch

echo "PASS: lifecycle suspend/resume CLI/socket integration smoke ($STORE)"
