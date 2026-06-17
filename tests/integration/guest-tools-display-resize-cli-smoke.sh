#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-display-resize.XXXXXX")"
SOCKET="$STORE/guest-tools.sock"
DISPLAY_COMMAND="$STORE/resize-display.sh"
DISPLAY_OUTPUT="$STORE/display.txt"
SERVER_READY="$STORE/server.ready"
SERVER_LOG="$STORE/socket-server.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
TOKEN="display-resize-smoke-token"
WIDTH=1440
HEIGHT=900
SCALE=2
FAIL_WIDTH=800
FAIL_HEIGHT=600
FAIL_SCALE=3
FAIL_MESSAGE="display backend refused ${FAIL_WIDTH}x${FAIL_HEIGHT} scale ${FAIL_SCALE}"
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

cat >"$DISPLAY_COMMAND" <<SH
#!/usr/bin/env sh
if [ "\$1" = "$FAIL_WIDTH" ] && [ "\$2" = "$FAIL_HEIGHT" ] && [ "\$3" = "$FAIL_SCALE" ]; then
  echo "$FAIL_MESSAGE" >&2
  exit 42
fi
printf '%s %s %s' "\$1" "\$2" "\$3" > "$DISPLAY_OUTPUT"
SH
chmod +x "$DISPLAY_COMMAND"

python3 - "$SOCKET" "$SERVER_READY" "$TOKEN" "$WIDTH" "$HEIGHT" "$SCALE" "$FAIL_WIDTH" "$FAIL_HEIGHT" "$FAIL_SCALE" "$FAIL_MESSAGE" >"$SERVER_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys

(
    socket_path,
    ready_path,
    token,
    width,
    height,
    scale,
    fail_width,
    fail_height,
    fail_scale,
    fail_message,
) = sys.argv[1:11]
width = int(width)
height = int(height)
scale = int(scale)
fail_width = int(fail_width)
fail_height = int(fail_height)
fail_scale = int(fail_scale)

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
    capability.get("name") == "display-resize"
    for capability in hello_message.get("capabilities", [])
), hello

heartbeat = read_frame()
assert heartbeat.get("message") == "Heartbeat", heartbeat

write_frame(
    {
        "ResizeDisplay": {
            "width": width,
            "height": height,
            "scale": scale,
        }
    },
    "resize-host-1",
)
expect_result(
    "resize-host-1",
    True,
    message=f"display resized to {width}x{height} scale {scale}",
)

write_frame(
    {
        "ResizeDisplay": {
            "width": fail_width,
            "height": fail_height,
            "scale": fail_scale,
        }
    },
    "resize-host-failed",
)
failure = expect_result(
    "resize-host-failed",
    False,
    error_code="display-resize-failed",
)
failure_message = failure.get("message", "")
assert "display resize command" in failure_message, failure
assert "resize-display.sh failed" in failure_message, failure
assert "exit status" in failure_message, failure

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
  --display-resize-command "$DISPLAY_COMMAND" \
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

[[ -f "$DISPLAY_OUTPUT" ]] || fail "display resize command did not write output"
[[ "$(cat "$DISPLAY_OUTPUT")" == "$WIDTH $HEIGHT $SCALE" ]] \
  || fail "display resize command output did not match dimensions"

PRESERVE_STORE=0
echo "PASS: guest-tools display-resize live socket smoke ($STORE)"
