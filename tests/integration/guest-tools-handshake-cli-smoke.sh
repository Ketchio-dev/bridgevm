#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-guest-tools-handshake.XXXXXX")"
VM_LOCAL="guest-tools-handshake-local"
VM_SOCKET="guest-tools-handshake-socket"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""
PRESERVE_STORE=1

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
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
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

assert_guest_tools_capability() {
  local output="$1"
  local capability="$2"
  local enabled_by="$3"
  local label="$4"

  assert_contains "$output" "Capability: $capability" "$label"
  assert_contains "$output" "Max version: 1" "$label"
  assert_contains "$output" "Enabled by: $enabled_by" "$label"
}

read_token() {
  local token_path="$1"
  python3 - "$token_path" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as token_file:
    print(json.load(token_file)["token"])
PY
}

hello_json() {
  local token="$1"
  local capabilities="$2"
  python3 - "$token" "$capabilities" <<'PY'
import json
import sys

token = sys.argv[1]
capabilities = [
    {"name": name, "version": 1}
    for name in sys.argv[2].split(",")
    if name
]
print(json.dumps({
    "protocol_version": 1,
    "message": {
        "GuestHello": {
            "version": 1,
            "guest_os": "linux",
            "agent_version": "1.0.0",
            "capabilities": capabilities,
            "auth": {"kind": "tools_token", "token": token},
        }
    },
}, separators=(",", ":")))
PY
}

exercise_vm() {
  local label="$1"
  local vm="$2"
  local runner="$3"

  local token_output
  token_output="$("$runner" guest-tools token "$vm")"
  assert_contains "$token_output" "Guest tools token for $vm" "$label token"
  assert_contains "$token_output" "Token:" "$label token"
  assert_contains "$token_output" "Created:" "$label token"

  local token_path="$STORE/vms/$vm.vmbridge/metadata/guest-tools-token.json"
  local socket_path="$STORE/vms/$vm.vmbridge/metadata/guest-tools.sock"
  [[ -f "$token_path" ]] || fail "$label token metadata missing: $token_path"
  local token
  token="$(read_token "$token_path")"
  [[ -n "$token" ]] || fail "$label token was empty"

  local status_output
  status_output="$("$runner" guest-tools status "$vm")"
  assert_contains "$status_output" "Guest tools status for $vm" "$label status"
  assert_contains "$status_output" "Tools token created:" "$label status"
  assert_guest_tools_capability "$status_output" "heartbeat" "base protocol" "$label status"
  assert_guest_tools_capability "$status_output" "guest-ip" "network reporting" "$label status"
  assert_guest_tools_capability "$status_output" "time-sync" "clock sync" "$label status"
  assert_guest_tools_capability "$status_output" "guest-metrics" "diagnostics" "$label status"
  assert_guest_tools_capability "$status_output" "clipboard" "manifest.integration.clipboard" "$label status"
  assert_guest_tools_capability "$status_output" "display-resize" "manifest.integration.dynamicResolution" "$label status"
  assert_guest_tools_capability "$status_output" "shared-folders" "manifest.integration.sharedFolders" "$label status"
  assert_guest_tools_capability "$status_output" "agent-update" "manifest.security.signedAgentUpdates" "$label status"
  assert_contains "$status_output" "Approved shared folders: 0" "$label status"
  assert_not_contains "$status_output" "$token" "$label status"

  local default_command
  default_command="$("$runner" guest-tools linux-command "$vm")"
  assert_contains "$default_command" "bridgevm-tools-linux" "$label default linux-command"
  assert_contains "$default_command" "--device" "$label default linux-command"
  assert_contains "$default_command" "--token-file" "$label default linux-command"
  assert_contains "$default_command" "$token_path" "$label default linux-command"
  assert_contains "$default_command" "--capability" "$label default linux-command"
  assert_contains "$default_command" "heartbeat:1" "$label default linux-command"
  assert_contains "$default_command" "guest-ip:1" "$label default linux-command"
  assert_contains "$default_command" "time-sync:1" "$label default linux-command"
  assert_contains "$default_command" "guest-metrics:1" "$label default linux-command"
  assert_contains "$default_command" "clipboard:1" "$label default linux-command"
  assert_contains "$default_command" "display-resize:1" "$label default linux-command"
  assert_contains "$default_command" "shared-folders:1" "$label default linux-command"
  assert_contains "$default_command" "agent-update:1" "$label default linux-command"
  assert_not_contains "$default_command" "$token" "$label default linux-command"

  local custom_token_file="$STORE/custom-$vm-token.json"
  local custom_device="/tmp/bridgevm-$vm-tools-device"
  local device_command
  device_command="$("$runner" guest-tools linux-command "$vm" \
    --transport device \
    --token-file "$custom_token_file" \
    --device "$custom_device")"
  assert_contains "$device_command" "$custom_token_file" "$label custom device linux-command"
  assert_contains "$device_command" "$custom_device" "$label custom device linux-command"
  assert_not_contains "$device_command" "$token" "$label custom device linux-command"

  local socket_command
  socket_command="$("$runner" guest-tools linux-command "$vm" --transport socket)"
  assert_contains "$socket_command" "--socket" "$label socket linux-command"
  assert_contains "$socket_command" "$socket_path" "$label socket linux-command"
  assert_contains "$socket_command" "$token_path" "$label socket linux-command"
  assert_not_contains "$socket_command" "$token" "$label socket linux-command"

  local valid_hello
  valid_hello="$(hello_json "$token" "heartbeat,clipboard,display-resize,shared-folders,guest-metrics,agent-update,time-sync")"
  local accepted
  accepted="$("$runner" guest-tools accept-hello "$vm" --hello-json "$valid_hello")"
  assert_contains "$accepted" "Accepted guest tools session for $vm" "$label accept-hello"
  assert_contains "$accepted" "Guest OS: linux" "$label accept-hello"
  assert_contains "$accepted" "Agent version: 1.0.0" "$label accept-hello"
  assert_contains "$accepted" "Capability: heartbeat" "$label accept-hello"
  assert_contains "$accepted" "Capability: clipboard" "$label accept-hello"
  assert_contains "$accepted" "Capability: display-resize" "$label accept-hello"
  assert_contains "$accepted" "Capability: shared-folders" "$label accept-hello"
  assert_contains "$accepted" "Capability: guest-metrics" "$label accept-hello"
  assert_contains "$accepted" "Capability: agent-update" "$label accept-hello"
  assert_contains "$accepted" "Capability: time-sync" "$label accept-hello"

  local wrong_token_hello
  wrong_token_hello="$(hello_json "wrong-token" "heartbeat")"
  assert_fails_contains \
    "$label wrong token accept-hello" \
    "InvalidToolsToken" \
    "$runner" guest-tools accept-hello "$vm" --hello-json "$wrong_token_hello"

  local disallowed_hello
  disallowed_hello="$(hello_json "$token" "heartbeat,not-allowed")"
  assert_fails_contains \
    "$label disallowed capability accept-hello" \
    "CapabilityNotAllowed" \
    "$runner" guest-tools accept-hello "$vm" --hello-json "$disallowed_hello"
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for handshake JSON fixtures"

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
exercise_vm "local" "$VM_LOCAL" bridgevm

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

bridgevm_socket create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
exercise_vm "socket" "$VM_SOCKET" bridgevm_socket

PRESERVE_STORE=0
echo "PASS: guest-tools handshake CLI/socket smoke"
