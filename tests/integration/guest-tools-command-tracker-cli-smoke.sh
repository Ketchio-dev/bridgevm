#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bvm-gt-tracker.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="guest-tools-command-tracker"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
QEMU_LOG="$STORE/fake-qemu.log"
SERVER_LOG="$STORE/guest-tools-server.log"
SERVER_READY="$STORE/guest-tools-server.ready"
COMMAND_SEEN="$STORE/command-seen.json"
SEND_STRAY="$STORE/send-stray"
SEND_EXPECTED="$STORE/send-expected"
STRAY_SENT="$STORE/stray-sent"
EXPECTED_SENT="$STORE/expected-sent"
RUNTIME_METADATA="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-runtime.json"
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
    echo "Guest-tools server log: $SERVER_LOG" >&2
  fi
  if [[ -f "$QEMU_LOG" ]]; then
    echo "Fake QEMU log: $QEMU_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly contained '$needle'; got: $haystack" ;;
    *) ;;
  esac
}

assert_command_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2
  local output
  set +e
  output="$("$@" 2>&1)"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
  ASSERT_OUTPUT="$output"
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

PATH="$FAKE_BIN:$PATH" BRIDGEVM_FAKE_QEMU_LOG="$QEMU_LOG" \
  cargo run --quiet -p bridgevm-daemon -- \
    --store "$STORE" \
    --reconcile-interval-ms 600000 \
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
  "$COMMAND_SEEN" \
  "$SEND_STRAY" \
  "$SEND_EXPECTED" \
  "$STRAY_SENT" \
  "$EXPECTED_SENT" \
  >"$SERVER_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys
import time

socket_path, token_path, ready_path, command_seen_path, send_stray_path, send_expected_path, stray_sent_path, expected_sent_path = sys.argv[1:9]

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

def read_frame():
    line = b""
    while not line.endswith(b"\n"):
        chunk = connection.recv(1)
        if not chunk:
            raise AssertionError("unexpected EOF while reading frame")
        line += chunk
    return json.loads(line.decode("utf-8"))

def wait_for(path):
    while not os.path.exists(path):
        time.sleep(0.025)

write_frame({
    "GuestHello": {
        "version": 1,
        "guest_os": "linux",
        "agent_version": "tracker-smoke",
        "capabilities": [
            {"name": "heartbeat", "version": 1},
            {"name": "clipboard", "version": 1}
        ],
        "auth": {"kind": "tools_token", "token": token}
    }
})

command = read_frame()
assert command.get("protocol_version") == 1, command
assert command.get("request_id") == "dup-1", command
assert command.get("message") == {"SetClipboard": {"text": "first pending command"}}, command
with open(command_seen_path, "w", encoding="utf-8") as command_seen:
    json.dump(command, command_seen, separators=(",", ":"))
    command_seen.write("\n")

wait_for(send_stray_path)
write_frame({
    "CommandResult": {
        "request_id": "stray-1",
        "ok": True,
        "error_code": None,
        "message": "stray result must not satisfy dup-1"
    }
})
with open(stray_sent_path, "w", encoding="utf-8") as stray_sent:
    stray_sent.write("sent\n")

wait_for(send_expected_path)
write_frame({
    "CommandResult": {
        "request_id": "dup-1",
        "ok": True,
        "error_code": None,
        "message": "original command completed",
        "result": {
            "changed": True,
            "text_length": 21
        },
        "metadata": {
            "handler": "clipboard",
            "smoke": "command-tracker"
        }
    }
})
with open(expected_sent_path, "w", encoding="utf-8") as expected_sent:
    expected_sent.write("sent\n")

time.sleep(0.25)
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

connected=""
for _ in {1..100}; do
  connected="$(bridgevm_socket guest-tools status "$VM_NAME" 2>/dev/null || true)"
  if [[ "$connected" == *"Runtime connected: true"* ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before daemon connected"
  fi
  sleep 0.05
done
assert_contains "$connected" "Runtime connected: true" "guest-tools status"
assert_contains "$connected" "clipboard" "guest-tools status"

first_output="$(
  bridgevm_socket guest-tools set-clipboard "$VM_NAME" \
    --text "first pending command" \
    --request-id "dup-1"
)"
assert_contains "$first_output" "Guest tools command sent for $VM_NAME" "first command"
assert_contains "$first_output" "Request ID: dup-1" "first command"
assert_contains "$first_output" "Pending commands: 1" "first command"

for _ in {1..100}; do
  if [[ -f "$COMMAND_SEEN" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before observing first command"
  fi
  sleep 0.05
done
[[ -f "$COMMAND_SEEN" ]] || fail "guest-tools server did not observe first command"

assert_command_fails_contains \
  "duplicate pending request id" \
  "PendingRequestExists" \
  bridgevm_socket guest-tools set-clipboard "$VM_NAME" \
    --text "duplicate pending command" \
    --request-id "dup-1"
assert_contains "$ASSERT_OUTPUT" "dup-1" "duplicate pending request id"

touch "$SEND_STRAY"
for _ in {1..100}; do
  if [[ -f "$STRAY_SENT" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before sending stray result"
  fi
  sleep 0.05
done
[[ -f "$STRAY_SENT" ]] || fail "guest-tools server did not send stray result"
stray_status="$(bridgevm_socket guest-tools status "$VM_NAME")"
assert_contains "$stray_status" "Runtime connected: true" "status after stray result"
assert_not_contains "$stray_status" "stray-1" "status after stray result"
[[ -f "$RUNTIME_METADATA" ]] || fail "runtime metadata missing after stray result"
if grep -q 'stray-1' "$RUNTIME_METADATA"; then
  fail "stray CommandResult was recorded in guest-tools runtime metadata"
fi
for _ in {1..100}; do
  if grep -q 'UnexpectedCommandResult { request_id: "stray-1" }' "$DAEMON_LOG"; then
    break
  fi
  sleep 0.05
done
grep -q 'UnexpectedCommandResult { request_id: "stray-1" }' "$DAEMON_LOG" \
  || fail "daemon log did not report unexpected stray CommandResult"

touch "$SEND_EXPECTED"
for _ in {1..100}; do
  if [[ -f "$EXPECTED_SENT" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before sending expected result"
  fi
  sleep 0.05
done
[[ -f "$EXPECTED_SENT" ]] || fail "guest-tools server did not send expected result"
completed=""
for _ in {1..100}; do
  completed="$(bridgevm_socket guest-tools status "$VM_NAME" 2>/dev/null || true)"
  if [[ "$completed" == *"Runtime connected: true"* ]]; then
    metadata_check="$(python3 - "$STORE" "$VM_NAME" <<'PY'
import json
import pathlib
import sys

store, vm = sys.argv[1:3]
runtime_path = pathlib.Path(store) / "vms" / f"{vm}.vmbridge" / "metadata" / "guest-tools-runtime.json"
if not runtime_path.exists():
    raise SystemExit(1)
runtime = json.loads(runtime_path.read_text(encoding="utf-8"))
result = runtime.get("last_command_result") or {}
if (
    result.get("request_id") == "dup-1"
    and result.get("capability") == "clipboard"
    and result.get("ok") is True
    and result.get("message") == "original command completed"
    and (result.get("result") or {}).get("changed") is True
    and (result.get("result") or {}).get("text_length") == 21
    and (result.get("metadata") or {}).get("handler") == "clipboard"
    and (result.get("metadata") or {}).get("smoke") == "command-tracker"
):
    print("ok")
PY
)"
    if [[ "$metadata_check" == "ok" ]]; then
      break
    fi
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "guest-tools server exited before original command completed"
  fi
  sleep 0.05
done
[[ "${metadata_check:-}" == "ok" ]] || fail "original command result was not recorded honestly"

assert_contains "$completed" "Last command request ID: dup-1" "completed status"
assert_contains "$completed" "Last command result JSON:" "completed status"
assert_contains "$completed" '"changed": true' "completed status"
assert_contains "$completed" '"text_length": 21' "completed status"
assert_contains "$completed" "Last command metadata JSON:" "completed status"
assert_contains "$completed" '"handler": "clipboard"' "completed status"
assert_contains "$completed" '"smoke": "command-tracker"' "completed status"

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

wait "$SERVER_PID"
SERVER_PID=""

kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""

PRESERVE_STORE=0
echo "PASS: guest-tools command tracker CLI/socket negative-path smoke ($STORE)"
