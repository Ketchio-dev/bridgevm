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
unset BRIDGEVM_APPLE_VZ_RUNNER
unset BRIDGEVM_APPLE_VZ_ALLOW_REAL_START

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
assert_contains "$stopped_suspend" "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner" \
  "local stopped suspend rejection"

bridgevm start "$VM_NAME" >/dev/null

local_suspend="$(bridgevm suspend "$VM_NAME" 2>&1 || true)"
assert_contains "$local_suspend" "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner" \
  "local suspend no runner"

local_running_status="$(bridgevm status "$VM_NAME")"
assert_contains "$local_running_status" "State: running" "local running status after failed suspend"

local_resume="$(bridgevm resume "$VM_NAME" 2>&1 || true)"
assert_contains "$local_resume" "no saved Fast Mode state to resume from" "local resume no saved state"

local_stop_from_running="$(bridgevm stop "$VM_NAME")"
assert_contains "$local_stop_from_running" "Stopped $VM_NAME" "local stop running"

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
assert_contains "$socket_stopped_suspend" "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner" \
  "socket stopped suspend rejection"

socket_start="$(bridgevm_socket start "$VM_NAME")"
assert_contains "$socket_start" "Metadata state recorded for $VM_NAME (running)" "socket start"

socket_suspend="$(bridgevm_socket suspend "$VM_NAME" 2>&1 || true)"
assert_contains "$socket_suspend" "set BRIDGEVM_APPLE_VZ_RUNNER to a signed AppleVzRunner" \
  "socket suspend no runner"

socket_running_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_running_status" "$VM_NAME" "socket running status after failed suspend"
assert_contains "$socket_running_status" "running" "socket running status after failed suspend"

socket_resume="$(bridgevm_socket resume "$VM_NAME" 2>&1 || true)"
assert_contains "$socket_resume" "no saved Fast Mode state to resume from" "socket resume no saved state"

socket_running_status="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$socket_running_status" "$VM_NAME" "socket running status"
assert_contains "$socket_running_status" "running" "socket running status"

bridgevm_socket stop "$VM_NAME" >/dev/null
assert_no_backend_launch

echo "PASS: lifecycle suspend/resume CLI/socket integration smoke ($STORE)"
