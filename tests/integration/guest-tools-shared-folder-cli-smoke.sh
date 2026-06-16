#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-shared-folder.XXXXXX")"
SOCKET="$STORE/guest-tools.sock"
SERVER_READY="$STORE/server.ready"
SERVER_LOG="$STORE/socket-server.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
TOKEN="shared-folder-smoke-token"
SERVER_PID=""
TOOLS_PID=""
PRESERVE_STORE=1

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -f "$SERVER_LOG" ]]; then
    echo "Socket server log: $SERVER_LOG" >&2
  fi
  if [[ -f "$TOOLS_LOG" ]]; then
    echo "bridgevm-tools-linux log: $TOOLS_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${TOOLS_PID:-}" ]]; then
    kill "$TOOLS_PID" 2>/dev/null || true
    wait "$TOOLS_PID" 2>/dev/null || true
  fi
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for the socket harness"

python3 - "$SOCKET" "$SERVER_READY" "$TOKEN" >"$SERVER_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys

socket_path, ready_path, token = sys.argv[1:4]

try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(1)
with open(ready_path, "w", encoding="utf-8") as ready:
    ready.write("ready\n")

connection, _ = server.accept()

def read_frame():
    line = b""
    while not line.endswith(b"\n"):
        chunk = connection.recv(1)
        if not chunk:
            raise AssertionError("unexpected EOF while reading frame")
        line += chunk
    return json.loads(line.decode("utf-8"))

def write_frame(message, request_id):
    frame = {
        "protocol_version": 1,
        "request_id": request_id,
        "message": message,
    }
    connection.sendall((json.dumps(frame, separators=(",", ":")) + "\n").encode("utf-8"))

def expect_result(request_id, ok, error_code=None, message=None):
    frame = read_frame()
    result = frame.get("message", {}).get("CommandResult")
    assert isinstance(result, dict), frame
    assert result.get("request_id") == request_id, frame
    assert result.get("ok") is ok, frame
    assert result.get("error_code") == error_code, frame
    if message is not None:
        assert result.get("message") == message, frame
    return result

hello = read_frame()
hello_message = hello.get("message", {}).get("GuestHello")
assert hello.get("protocol_version") == 1, hello
assert hello_message is not None, hello
assert hello_message.get("guest_os") == "linux", hello
assert hello_message.get("auth") == {"kind": "tools_token", "token": token}, hello
assert any(
    capability.get("name") == "shared-folders"
    for capability in hello_message.get("capabilities", [])
), hello

heartbeat = read_frame()
assert heartbeat.get("message") == "Heartbeat", heartbeat

write_frame(
    {
        "MountShare": {
            "name": "workspace",
            "host_path_token": "host-token-workspace",
        }
    },
    "mount-workspace",
)
expect_result(
    "mount-workspace",
    True,
    message="accepted mount request for share workspace",
)

write_frame(
    {
        "UnmountShare": {
            "name": "workspace",
        }
    },
    "unmount-workspace",
)
expect_result(
    "unmount-workspace",
    True,
    message="accepted unmount request for share workspace",
)

write_frame(
    {
        "UnmountShare": {
            "name": "workspace",
        }
    },
    "unmount-missing-workspace",
)
expect_result(
    "unmount-missing-workspace",
    False,
    "share-not-mounted",
    "share workspace is not mounted",
)

connection.close()
server.close()
PY
SERVER_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" && -f "$SERVER_READY" ]]; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    fail "socket server exited before becoming ready"
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "guest-tools socket was not ready"

cargo run --quiet -p bridgevm-tools-linux -- \
  --socket "$SOCKET" \
  --token "$TOKEN" \
  --no-guest-ip \
  --no-metrics \
  >"$TOOLS_LOG" 2>&1 &
TOOLS_PID=$!

if ! wait "$SERVER_PID"; then
  SERVER_PID=""
  fail "socket server harness failed"
fi
SERVER_PID=""

if ! wait "$TOOLS_PID"; then
  TOOLS_PID=""
  fail "bridgevm-tools-linux exited with an error"
fi
TOOLS_PID=""

PRESERVE_STORE=0
echo "PASS: guest-tools shared-folder live socket smoke ($STORE)"
