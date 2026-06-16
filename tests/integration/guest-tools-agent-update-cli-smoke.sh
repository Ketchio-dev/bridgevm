#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bvm-agent-update.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="guest-tools-agent-update"
UNSIGNED_VM_NAME="guest-tools-no-signed"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
QEMU_LOG="$STORE/fake-qemu.log"
SERVER_LOG="$STORE/guest-tools-agent-update-server.log"
SERVER_READY="$STORE/guest-tools-agent-update-server.ready"
UPDATE_SENT="$STORE/agent-update-sent"
SERVER_STOP="$STORE/agent-update-stop"
DAEMON_PID=""
SERVER_PID=""
PRESERVE_STORE=1

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
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
  if [[ -f "$SERVER_LOG" ]]; then
    echo "Guest-tools agent-update server log: $SERVER_LOG" >&2
  fi
  if [[ -f "$QEMU_LOG" ]]; then
    echo "Fake QEMU log: $QEMU_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    touch "$SERVER_STOP" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
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

assert_not_contains_lower() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  local lower
  lower="$(printf "%s" "$haystack" | tr '[:upper:]' '[:lower:]')"
  case "$lower" in
    *"$needle"*) fail "$label unexpectedly contained '$needle'; got: $haystack" ;;
    *) ;;
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

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for the socket harness"

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    output="${@: -2:1}"
    printf 'fake qcow2\n' >"$output"
    ;;
  info)
    printf '{"format":"qcow2","virtual-size":85899345920}\n'
    ;;
  *)
    echo "unsupported fake qemu-img command: $*" >&2
    exit 1
    ;;
esac
SH
chmod +x "$FAKE_BIN/qemu-img"

cat >"$FAKE_BIN/qemu-system-x86_64" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

echo "fake qemu-system-x86_64 started: $*" >>"${BRIDGEVM_FAKE_QEMU_LOG:?}"
trap 'exit 0' TERM INT
while true; do
  sleep 1
done
SH
chmod +x "$FAKE_BIN/qemu-system-x86_64"

PATH="$FAKE_BIN:$PATH" bridgevm create "$VM_NAME" \
  --os ubuntu \
  --arch x86_64 \
  --mode compatibility >/dev/null
PATH="$FAKE_BIN:$PATH" bridgevm disk create "$VM_NAME" >/dev/null

policy_status="$(bridgevm guest-tools status "$VM_NAME")"
assert_contains "$policy_status" "Capability: agent-update" "guest-tools policy"
assert_contains \
  "$policy_status" \
  "Enabled by: manifest.security.signedAgentUpdates" \
  "guest-tools policy"

PATH="$FAKE_BIN:$PATH" bridgevm create "$UNSIGNED_VM_NAME" \
  --os ubuntu \
  --arch x86_64 \
  --mode compatibility >/dev/null
unsigned_manifest="$STORE/vms/$UNSIGNED_VM_NAME.vmbridge/manifest.yaml"
sed -i.bak 's/signedAgentUpdates: true/signedAgentUpdates: false/' "$unsigned_manifest"
grep -q "signedAgentUpdates: false" "$unsigned_manifest" \
  || fail "unsigned guest-tools policy fixture did not disable signedAgentUpdates"

unsigned_policy_status="$(bridgevm guest-tools status "$UNSIGNED_VM_NAME")"
assert_not_contains \
  "$unsigned_policy_status" \
  "Capability: agent-update" \
  "guest-tools unsigned policy"
assert_not_contains \
  "$unsigned_policy_status" \
  "Enabled by: manifest.security.signedAgentUpdates" \
  "guest-tools unsigned policy"

PATH="$FAKE_BIN:$PATH" BRIDGEVM_FAKE_QEMU_LOG="$QEMU_LOG" \
  cargo run --quiet -p bridgevm-daemon -- \
    --store "$STORE" \
    --reconcile-interval-ms 25 \
    >"$DAEMON_LOG" 2>&1 &
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

PATH="$FAKE_BIN:$PATH" bridgevm_socket run "$VM_NAME" --spawn >/dev/null

GUEST_TOOLS_SOCKET="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools.sock"
GUEST_TOOLS_TOKEN="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-token.json"
[[ -f "$GUEST_TOOLS_TOKEN" ]] || fail "guest-tools token metadata missing"

python3 - \
  "$GUEST_TOOLS_SOCKET" \
  "$GUEST_TOOLS_TOKEN" \
  "$SERVER_READY" \
  "$UPDATE_SENT" \
  "$SERVER_STOP" \
  >"$SERVER_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys
import time

socket_path, token_path, ready_path, update_sent_path, stop_path = sys.argv[1:6]

with open(token_path, "r", encoding="utf-8") as token_file:
    token = json.load(token_file)["token"]

try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

os.makedirs(os.path.dirname(socket_path), exist_ok=True)
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(1)
with open(ready_path, "w", encoding="utf-8") as ready:
    ready.write("ready\n")

connection, _ = server.accept()

def write_frame(message, request_id=None):
    frame = {
        "protocol_version": 1,
        "request_id": request_id,
        "message": message,
    }
    connection.sendall((json.dumps(frame, separators=(",", ":")) + "\n").encode("utf-8"))

write_frame({
    "GuestHello": {
        "version": 1,
        "guest_os": "linux",
        "agent_version": "1.2.3",
        "capabilities": [
            {"name": "heartbeat", "version": 1},
            {"name": "agent-update", "version": 1}
        ],
        "auth": {"kind": "tools_token", "token": token}
    }
})
write_frame("Heartbeat")
write_frame({
    "AgentUpdateAvailable": {
        "current_version": "1.2.3",
        "available_version": "1.2.4",
        "download_url": "https://updates.example.invalid/bridgevm-tools-linux-1.2.4.tar.zst",
        "signature": "minisig:agent-update-smoke-signature"
    }
}, "agent-update-available-1")

with open(update_sent_path, "w", encoding="utf-8") as update_sent:
    update_sent.write("sent\n")

while not os.path.exists(stop_path):
    time.sleep(0.05)

connection.close()
server.close()
PY
SERVER_PID=$!

for _ in {1..100}; do
  if [[ -S "$GUEST_TOOLS_SOCKET" && -f "$SERVER_READY" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before becoming ready"
  fi
  sleep 0.05
done
[[ -S "$GUEST_TOOLS_SOCKET" ]] || fail "guest-tools socket was not ready"

for _ in {1..100}; do
  if [[ -f "$UPDATE_SENT" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before sending AgentUpdateAvailable"
  fi
  sleep 0.05
done
[[ -f "$UPDATE_SENT" ]] || fail "fake guest did not send AgentUpdateAvailable"

runtime_metadata="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-runtime.json"
for _ in {1..100}; do
  if python3 - "$runtime_metadata" <<'PY'
import json
import sys

path = sys.argv[1]
try:
    with open(path, "r", encoding="utf-8") as handle:
        runtime = json.load(handle)
except FileNotFoundError:
    sys.exit(1)

update = runtime.get("agent_update")
if not isinstance(update, dict):
    sys.exit(1)

checks = [
    runtime.get("connected") is True,
    "agent-update" in runtime.get("capabilities", []),
    update.get("current_version") == "1.2.3",
    update.get("available_version") == "1.2.4",
    update.get("download_url") == "https://updates.example.invalid/bridgevm-tools-linux-1.2.4.tar.zst",
    update.get("signature") == "minisig:agent-update-smoke-signature",
    isinstance(update.get("observed_at_unix"), int),
]
sys.exit(0 if all(checks) else 1)
PY
  then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before agent-update metadata was recorded"
  fi
  sleep 0.05
done

python3 - "$runtime_metadata" <<'PY' \
  || fail "AgentUpdateAvailable runtime metadata was not recorded with the expected passive fields"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    runtime = json.load(handle)

update = runtime.get("agent_update") or {}
assert runtime.get("connected") is True, runtime
assert "agent-update" in runtime.get("capabilities", []), runtime
assert update.get("current_version") == "1.2.3", update
assert update.get("available_version") == "1.2.4", update
assert update.get("download_url") == "https://updates.example.invalid/bridgevm-tools-linux-1.2.4.tar.zst", update
assert update.get("signature") == "minisig:agent-update-smoke-signature", update
assert isinstance(update.get("observed_at_unix"), int), update
PY

status_output="$(bridgevm_socket guest-tools status "$VM_NAME")"
assert_contains "$status_output" "Runtime connected: true" "guest-tools status"
assert_contains "$status_output" "Capability: agent-update" "guest-tools status"
assert_contains "$status_output" "Agent update current: 1.2.3" "guest-tools status"
assert_contains "$status_output" "Agent update available: 1.2.4" "guest-tools status"
assert_contains \
  "$status_output" \
  "Agent update URL: https://updates.example.invalid/bridgevm-tools-linux-1.2.4.tar.zst" \
  "guest-tools status"
assert_contains \
  "$status_output" \
  "Agent update signature: present" \
  "guest-tools status"
assert_contains "$status_output" "Agent update observed:" "guest-tools status"
assert_not_contains_lower "$status_output" "downloaded" "guest-tools status"
assert_not_contains_lower "$status_output" "installed" "guest-tools status"
assert_not_contains_lower "$status_output" "executed" "guest-tools status"
assert_not_contains_lower "$status_output" "auto-update completed" "guest-tools status"

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

touch "$SERVER_STOP"
wait "$SERVER_PID"
SERVER_PID=""

kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""

PRESERVE_STORE=0
echo "PASS: guest-tools agent-update passive metadata smoke ($STORE)"
