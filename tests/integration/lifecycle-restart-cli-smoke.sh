#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-lifecycle.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="lifecycle-linux"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

for backend in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in lifecycle restart smoke: $(basename "$0")" >&2
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

write_state_fixture() {
  local state="$1"
  local metadata_dir="$STORE/vms/$VM_NAME.vmbridge/metadata"
  mkdir -p "$metadata_dir"
  cat >"$metadata_dir/state.json" <<EOF
{
  "state": "$state",
  "updated_at_unix": 1
}
EOF
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode fast >/dev/null

bridgevm start "$VM_NAME" >/dev/null
local_restart="$(bridgevm restart "$VM_NAME")"
assert_contains "$local_restart" "Stopped $VM_NAME" "local restart"
assert_contains "$local_restart" "Metadata state recorded for $VM_NAME (running)" "local restart"

local_status="$(bridgevm status "$VM_NAME")"
assert_contains "$local_status" "Name: $VM_NAME" "local status after restart"
assert_contains "$local_status" "State: running" "local status after restart"

write_state_fixture "suspended"
local_restart_from_suspend="$(bridgevm restart "$VM_NAME")"
assert_contains "$local_restart_from_suspend" "Metadata state recorded for $VM_NAME (running)" "local suspended restart"
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

socket_restart="$(bridgevm_socket restart "$VM_NAME")"
assert_contains "$socket_restart" "Metadata state recorded for $VM_NAME (running)" "socket restart"

socket_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_status" "$VM_NAME" "socket status after restart"
assert_contains "$socket_status" "running" "socket status after restart"

write_state_fixture "suspended"
socket_restart_from_suspend="$(bridgevm_socket restart "$VM_NAME")"
assert_contains "$socket_restart_from_suspend" "Metadata state recorded for $VM_NAME (running)" "socket suspended restart"

socket_suspended_restart_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_suspended_restart_status" "$VM_NAME" "socket status after suspended restart"
assert_contains "$socket_suspended_restart_status" "running" "socket status after suspended restart"

assert_no_backend_launch

echo "PASS: lifecycle restart CLI/socket integration smoke ($STORE)"
