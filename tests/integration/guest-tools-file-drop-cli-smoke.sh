#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-file-drop.XXXXXX")"
SOCKET="$STORE/guest-tools.sock"
DROP_DIR="$STORE/drops"
EXPECTED_FILE="$STORE/expected-notes.txt"
SERVER_READY="$STORE/server.ready"
SERVER_LOG="$STORE/socket-server.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
TOKEN="file-drop-smoke-token"
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
import base64
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

def expect_result(request_id, ok, error_code=None):
    frame = read_frame()
    message = frame.get("message")
    assert isinstance(message, dict), frame
    result = message.get("CommandResult")
    assert isinstance(result, dict), frame
    assert result.get("request_id") == request_id, frame
    assert result.get("ok") is ok, frame
    assert result.get("error_code") == error_code, frame
    return result

hello = read_frame()
hello_message = hello.get("message", {}).get("GuestHello")
assert hello.get("protocol_version") == 1, hello
assert hello_message is not None, hello
assert hello_message.get("guest_os") == "linux", hello
assert hello_message.get("auth") == {"kind": "tools_token", "token": token}, hello
assert any(
    capability.get("name") == "drag-drop"
    for capability in hello_message.get("capabilities", [])
), hello

heartbeat = read_frame()
assert heartbeat.get("message") == "Heartbeat", heartbeat

payload = b"hello from socket file drop\n"
chunks = [payload[:11], payload[11:]]

write_frame(
    {
        "FileDropStart": {
            "transfer_id": "drop-ok",
            "file_name": "notes.txt",
            "size_bytes": len(payload),
        }
    },
    "drop-ok-start",
)
expect_result("drop-ok-start", True)

for index, chunk in enumerate(chunks):
    write_frame(
        {
            "FileDropChunk": {
                "transfer_id": "drop-ok",
                "chunk_index": index,
                "data_base64": base64.b64encode(chunk).decode("ascii"),
            }
        },
        f"drop-ok-chunk-{index}",
    )
    expect_result(f"drop-ok-chunk-{index}", True)

write_frame({"FileDropComplete": {"transfer_id": "drop-ok"}}, "drop-ok-complete")
expect_result("drop-ok-complete", True)

unsafe_payload = b"data"
write_frame(
    {
        "FileDropStart": {
            "transfer_id": "drop-unsafe",
            "file_name": "../escape.txt",
            "size_bytes": len(unsafe_payload),
        }
    },
    "drop-unsafe-start",
)
expect_result("drop-unsafe-start", True)
write_frame(
    {
        "FileDropChunk": {
            "transfer_id": "drop-unsafe",
            "chunk_index": 0,
            "data_base64": base64.b64encode(unsafe_payload).decode("ascii"),
        }
    },
    "drop-unsafe-chunk",
)
expect_result("drop-unsafe-chunk", True)
write_frame(
    {"FileDropComplete": {"transfer_id": "drop-unsafe"}},
    "drop-unsafe-complete",
)
expect_result("drop-unsafe-complete", False, "unsafe-file-name")

mismatch_payload = b"tiny"
write_frame(
    {
        "FileDropStart": {
            "transfer_id": "drop-mismatch",
            "file_name": "short.txt",
            "size_bytes": len(mismatch_payload) + 1,
        }
    },
    "drop-mismatch-start",
)
expect_result("drop-mismatch-start", True)
write_frame(
    {
        "FileDropChunk": {
            "transfer_id": "drop-mismatch",
            "chunk_index": 0,
            "data_base64": base64.b64encode(mismatch_payload).decode("ascii"),
        }
    },
    "drop-mismatch-chunk",
)
expect_result("drop-mismatch-chunk", True)
write_frame(
    {"FileDropComplete": {"transfer_id": "drop-mismatch"}},
    "drop-mismatch-complete",
)
expect_result("drop-mismatch-complete", False, "transfer-size-mismatch")

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
  --file-drop-dir "$DROP_DIR" \
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

printf "hello from socket file drop\n" >"$EXPECTED_FILE"
[[ -f "$DROP_DIR/notes.txt" ]] || fail "file-drop output file was not written"
cmp -s "$EXPECTED_FILE" "$DROP_DIR/notes.txt" \
  || fail "file-drop output file content differed"

[[ ! -e "$STORE/escape.txt" ]] || fail "unsafe file name wrote outside drop directory"
[[ ! -e "$DROP_DIR/escape.txt" ]] || fail "unsafe file name wrote inside drop directory"
[[ ! -e "$DROP_DIR/short.txt" ]] || fail "size mismatch wrote an output file"

PRESERVE_STORE=0
echo "PASS: guest-tools file-drop live socket smoke ($STORE)"
