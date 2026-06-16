#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bvm-fsfreeze.XXXXXX")"
FAKE_BIN="$STORE/bin"
SOCKET="$STORE/guest-tools.sock"
SERVER_READY="$STORE/server.ready"
SERVER_LOG="$STORE/socket-server.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
FSFREEZE_LOG="$STORE/fsfreeze.log"
MOUNT_ONE="$STORE/mounts/root"
MOUNT_TWO="$STORE/mounts/data"
TOKEN="fsfreeze-backend-smoke-token"
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
  if [[ -f "$FSFREEZE_LOG" ]]; then
    echo "fake fsfreeze log: $FSFREEZE_LOG" >&2
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

assert_file_equals() {
  local path="$1"
  local expected="$2"
  local label="$3"
  if [[ ! -f "$path" ]]; then
    fail "$label missing file $path"
  fi
  local actual
  actual="$(cat "$path")"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label mismatch; expected '$expected' got '$actual'"
  fi
}

run_tools_case() {
  local label="$1"
  local mode="$2"
  local fail_mount="${3:-}"

  rm -f "$SOCKET" "$SERVER_READY" "$SERVER_LOG" "$TOOLS_LOG" "$FSFREEZE_LOG"

  python3 - "$SOCKET" "$SERVER_READY" "$TOKEN" "$mode" "$MOUNT_ONE" "$MOUNT_TWO" \
    >"$SERVER_LOG" 2>&1 <<'PY' &
import json
import os
import socket
import sys

socket_path, ready_path, token, mode, mount_one, mount_two = sys.argv[1:7]

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

def expect_result(request_id, ok, error_code=None, message_contains=None):
    frame = read_frame()
    message = frame.get("message")
    assert isinstance(message, dict), frame
    result = message.get("CommandResult")
    assert isinstance(result, dict), frame
    assert result.get("request_id") == request_id, frame
    assert result.get("ok") is ok, frame
    assert result.get("error_code") == error_code, frame
    if message_contains is not None:
        assert message_contains in (result.get("message") or ""), frame
    return result

hello = read_frame()
hello_message = hello.get("message", {}).get("GuestHello")
assert hello.get("protocol_version") == 1, hello
assert hello_message is not None, hello
assert hello_message.get("guest_os") == "linux", hello
assert hello_message.get("auth") == {"kind": "tools_token", "token": token}, hello
capabilities = hello_message.get("capabilities", [])
assert any(capability.get("name") == "fs-freeze" for capability in capabilities), hello
assert any(capability.get("name") == "fs-thaw" for capability in capabilities), hello

heartbeat = read_frame()
assert heartbeat.get("message") == "Heartbeat", heartbeat

if mode == "success":
    write_frame({"FreezeFilesystem": {"timeout_millis": 10000}}, "freeze-ok")
    expect_result("freeze-ok", True, message_contains=f"{mount_one}, {mount_two}")
    write_frame("ThawFilesystem", "thaw-ok")
    expect_result("thaw-ok", True, message_contains=f"{mount_one}, {mount_two}")
elif mode == "partial-failure":
    write_frame({"FreezeFilesystem": {"timeout_millis": 10000}}, "freeze-fail")
    expect_result(
        "freeze-fail",
        False,
        "filesystem-freeze-failed",
        f"failed to freeze {mount_two}",
    )
    write_frame("ThawFilesystem", "thaw-after-fail")
    expect_result(
        "thaw-after-fail",
        False,
        "filesystem-not-frozen",
        "filesystem thaw scaffold boundary is not active",
    )
else:
    raise AssertionError(f"unknown mode {mode}")

connection.close()
server.close()
PY
  SERVER_PID=$!

  for _ in {1..100}; do
    if [[ -S "$SOCKET" && -f "$SERVER_READY" ]]; then
      break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      fail "$label socket harness exited before becoming ready"
    fi
    sleep 0.05
  done
  [[ -S "$SOCKET" ]] || fail "$label socket was not ready"

  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_FSFREEZE_LOG="$FSFREEZE_LOG" \
  BRIDGEVM_FAKE_FSFREEZE_FAIL_MOUNT="$fail_mount" \
    cargo run --quiet -p bridgevm-tools-linux -- \
      --socket "$SOCKET" \
      --token "$TOKEN" \
      --capability heartbeat:1 \
      --capability fs-freeze:1 \
      --capability fs-thaw:1 \
      --real-fsfreeze \
      --fsfreeze-mount "$MOUNT_ONE" \
      --fsfreeze-mount "$MOUNT_TWO" \
      >"$TOOLS_LOG" 2>&1 &
  TOOLS_PID=$!

  wait "$SERVER_PID" || fail "$label socket harness failed"
  SERVER_PID=""
  wait "$TOOLS_PID" || fail "$label bridgevm-tools-linux failed"
  TOOLS_PID=""
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for the socket harness"

mkdir -p "$FAKE_BIN" "$MOUNT_ONE" "$MOUNT_TWO"
cat >"$FAKE_BIN/fsfreeze" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  -f) action="freeze" ;;
  -u) action="thaw" ;;
  *)
    echo "unsupported fake fsfreeze flag: ${1:-}" >&2
    exit 64
    ;;
esac

mount="${2:-}"
if [[ -z "$mount" ]]; then
  echo "missing fake fsfreeze mount" >&2
  exit 64
fi

printf '%s %s\n' "$action" "$mount" >>"${BRIDGEVM_FAKE_FSFREEZE_LOG:?}"

if [[ "$action" == "freeze" && "${BRIDGEVM_FAKE_FSFREEZE_FAIL_MOUNT:-}" == "$mount" ]]; then
  echo "injected freeze failure for $mount" >&2
  exit 42
fi
SH
chmod +x "$FAKE_BIN/fsfreeze"

resolved_fsfreeze="$(PATH="$FAKE_BIN:$PATH" command -v fsfreeze)"
[[ "$resolved_fsfreeze" == "$FAKE_BIN/fsfreeze" ]] \
  || fail "fake fsfreeze was not first on PATH: $resolved_fsfreeze"
case "$MOUNT_ONE:$MOUNT_TWO" in
  "$STORE"/*:"$STORE"/*) ;;
  *) fail "fake fsfreeze mounts must stay inside disposable store" ;;
esac

run_tools_case "successful real-fsfreeze command-path with fake backend" "success"
assert_file_equals "$FSFREEZE_LOG" \
  "freeze $MOUNT_ONE
freeze $MOUNT_TWO
thaw $MOUNT_TWO
thaw $MOUNT_ONE" \
  "successful real-fsfreeze fake-command call order"

run_tools_case "partial real-fsfreeze command-path with fake backend failure" "partial-failure" "$MOUNT_TWO"
assert_file_equals "$FSFREEZE_LOG" \
  "freeze $MOUNT_ONE
freeze $MOUNT_TWO
thaw $MOUNT_ONE" \
  "partial real-fsfreeze fake-command rollback order"

PRESERVE_STORE=0
echo "PASS: application-consistent real-fsfreeze command-path fake backend smoke ($STORE)"
