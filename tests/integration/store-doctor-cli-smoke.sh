#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-store-doctor.XXXXXX")"
FAKE_BIN="$STORE/bin"
FAKE_EXEC_LOG="$STORE/fake-exec.log"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""

bridgevm() {
  PATH="$FAKE_BIN:$PATH" cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  PATH="$FAKE_BIN:$PATH" cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  PATH="$FAKE_BIN:$PATH" cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  if [[ -f "$FAKE_EXEC_LOG" ]]; then
    echo "Fake executable log: $FAKE_EXEC_LOG" >&2
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

assert_no_fake_execution() {
  [[ ! -s "$FAKE_EXEC_LOG" ]] || fail "doctor executed a host tool: $(cat "$FAKE_EXEC_LOG")"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

mkdir -p "$FAKE_BIN"
for tool in qemu-img qemu-system-aarch64 qemu-system-x86_64 lightvm-runner fullvm-runner networkd; do
  cat >"$FAKE_BIN/$tool" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_EXEC_LOG:?}"
echo "store doctor smoke forbids executing $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$tool"
done
export BRIDGEVM_FAKE_EXEC_LOG="$FAKE_EXEC_LOG"

assert_doctor_contract() {
  local label="$1"
  local output="$2"

  assert_contains "$output" "BridgeVM store: $STORE" "$label"
  assert_contains "$output" "VM bundles: $STORE/vms" "$label"
  assert_contains "$output" "Host capability audit:" "$label"
  assert_contains "$output" "[OK] Store root: $STORE exists" "$label"
  assert_contains "$output" "[OK] VM bundles dir: $STORE/vms exists" "$label"
  assert_contains "$output" "[OK] qemu-img: found at $FAKE_BIN/qemu-img" "$label"
  assert_contains "$output" "[OK] QEMU system binary: found qemu-system-aarch64 at $FAKE_BIN/qemu-system-aarch64 and qemu-system-x86_64 at $FAKE_BIN/qemu-system-x86_64" "$label"
  assert_contains "$output" "[OK] lightvm-runner: found at $FAKE_BIN/lightvm-runner" "$label"
  assert_contains "$output" "[OK] fullvm-runner: found at $FAKE_BIN/fullvm-runner" "$label"
  assert_contains "$output" "[OK] networkd: found at $FAKE_BIN/networkd" "$label"
  assert_contains "$output" "macOS host:" "$label"
  assert_contains "$output" "Apple Silicon host:" "$label"
  assert_contains "$output" "Fast Mode possibility:" "$label"
  assert_contains "$output" "Status: OK" "$label"
}

local_output="$(bridgevm doctor)"
assert_doctor_contract "local doctor" "$local_output"
assert_no_fake_execution

local_store_output="$(bridgevm store doctor)"
assert_doctor_contract "local store doctor" "$local_store_output"
assert_no_fake_execution

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon exited before socket became ready"
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

socket_output="$(bridgevm_socket doctor)"
assert_doctor_contract "socket doctor" "$socket_output"
assert_no_fake_execution

socket_store_output="$(bridgevm_socket store doctor)"
assert_doctor_contract "socket store doctor" "$socket_store_output"
assert_no_fake_execution

echo "PASS: store doctor CLI/socket metadata smoke ($STORE)"
