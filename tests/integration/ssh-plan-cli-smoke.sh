#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-ssh-plan.XXXXXX")"
VM_NAME="legacy-ssh"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
RUNTIME="$BUNDLE/metadata/guest-tools-runtime.json"
FAKE_BIN="$STORE/bin"
SSH_MARKER="$STORE/ssh-invoked"

mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/ssh" <<'SH'
#!/usr/bin/env bash
echo "$*" >"${BRIDGEVM_SSH_MARKER:?}"
exit 42
SH
chmod +x "$FAKE_BIN/ssh"
export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_SSH_MARKER="$SSH_MARKER"

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

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

assert_ssh_not_invoked() {
  local label="$1"
  if [[ -e "$SSH_MARKER" ]]; then
    fail "$label invoked host ssh command: $(cat "$SSH_MARKER")"
  fi
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

assert_fails_contains \
  "local missing ssh target" \
  "no SSH target available" \
  bridgevm ssh "$VM_NAME" --user ubuntu
assert_ssh_not_invoked "local missing ssh target"
assert_fails_contains \
  "local empty ssh user" \
  "ssh user must not be empty" \
  bridgevm ssh "$VM_NAME" --user ""
assert_ssh_not_invoked "local empty ssh user"

bridgevm port add "$VM_NAME" 2222:22 >/dev/null
bridgevm port add "$VM_NAME" 2200:22 >/dev/null

local_forward="$(bridgevm ssh "$VM_NAME" --user ubuntu)"
assert_contains "$local_forward" "SSH target for $VM_NAME" "local forward ssh plan"
assert_contains "$local_forward" "Source: PortForward" "local forward ssh plan"
assert_contains "$local_forward" "Host: 127.0.0.1" "local forward ssh plan"
assert_contains "$local_forward" "Port: 2200" "local forward ssh plan"
assert_contains "$local_forward" "User: ubuntu" "local forward ssh plan"
assert_contains "$local_forward" "Command: ssh -p 2200 ubuntu@127.0.0.1" "local forward ssh plan"
assert_ssh_not_invoked "local forward ssh plan"

mkdir -p "$BUNDLE/metadata"
python3 - "$RUNTIME" <<'PY'
import json
import os
import sys

runtime_path = sys.argv[1]
tmp_path = f"{runtime_path}.tmp"
metadata = {
    "connected": True,
    "guest_os": "linux",
    "agent_version": "0.1.0",
    "capabilities": ["heartbeat", "guest-ip"],
    "last_heartbeat_at_unix": 1,
    "guest_ip_addresses": [
        {"address": "169.254.1.10", "interface": "linklocal0"},
        {"address": "10.0.2.15", "interface": "eth0"},
    ],
    "shared_folders": [],
    "metrics": None,
    "updated_at_unix": 2,
}
with open(tmp_path, "w", encoding="utf-8") as handle:
    json.dump(metadata, handle, indent=2, sort_keys=True)
    handle.write("\n")
os.replace(tmp_path, runtime_path)
PY

local_forward_preferred="$(bridgevm ssh "$VM_NAME" --user ubuntu)"
assert_contains "$local_forward_preferred" "Source: PortForward" "local forward preferred"
assert_contains "$local_forward_preferred" "Command: ssh -p 2200 ubuntu@127.0.0.1" "local forward preferred"
assert_ssh_not_invoked "local forward preferred"

bridgevm port remove "$VM_NAME" 2200:22 >/dev/null
bridgevm port remove "$VM_NAME" 2222:22 >/dev/null

local_guest_ip="$(bridgevm ssh "$VM_NAME" --user ubuntu)"
assert_contains "$local_guest_ip" "Source: GuestToolsIp" "local guest ip ssh plan"
assert_contains "$local_guest_ip" "Host: 10.0.2.15" "local guest ip ssh plan"
assert_contains "$local_guest_ip" "Port: 22" "local guest ip ssh plan"
assert_contains "$local_guest_ip" "Command: ssh ubuntu@10.0.2.15" "local guest ip ssh plan"
assert_ssh_not_invoked "local guest ip ssh plan"

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

NO_TARGET_VM="socket-no-ssh-target"
bridgevm create "$NO_TARGET_VM" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
assert_fails_contains \
  "socket missing ssh target" \
  "no SSH target available" \
  bridgevm_socket ssh "$NO_TARGET_VM" --user ubuntu
assert_ssh_not_invoked "socket missing ssh target"
assert_fails_contains \
  "socket empty ssh user" \
  "ssh user must not be empty" \
  bridgevm_socket ssh "$VM_NAME" --user ""
assert_ssh_not_invoked "socket empty ssh user"

socket_guest_ip="$(bridgevm_socket ssh "$VM_NAME" --user ubuntu)"
assert_contains "$socket_guest_ip" "Source: GuestToolsIp" "socket guest ip ssh plan"
assert_contains "$socket_guest_ip" "Command: ssh ubuntu@10.0.2.15" "socket guest ip ssh plan"
assert_ssh_not_invoked "socket guest ip ssh plan"

bridgevm_socket port add "$VM_NAME" 2222:22 >/dev/null
bridgevm_socket port add "$VM_NAME" 2200:22 >/dev/null

socket_forward="$(bridgevm_socket ssh "$VM_NAME" --user ubuntu)"
assert_contains "$socket_forward" "Source: PortForward" "socket forward ssh plan"
assert_contains "$socket_forward" "Port: 2200" "socket forward ssh plan"
assert_contains "$socket_forward" "Command: ssh -p 2200 ubuntu@127.0.0.1" "socket forward ssh plan"
assert_ssh_not_invoked "socket forward ssh plan"

echo "PASS: ssh plan CLI/socket integration smoke ($STORE)"
