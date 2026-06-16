#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-time-sync.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="time-sync-live"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
QEMU_LOG="$STORE/fake-qemu.log"
PROXY_SOCKET="$STORE/tools-proxy.sock"
PROXY_READY="$STORE/tools-proxy.ready"
PROXY_LOG="$STORE/tools-proxy.log"
EXPECTED_MILLIS=1712345678000
REQUEST_ID="time-sync-alpha-1"
DAEMON_PID=""
TOOLS_PID=""
PROXY_PID=""
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
  if [[ -f "$TOOLS_LOG" ]]; then
    echo "bridgevm-tools-linux log: $TOOLS_LOG" >&2
  fi
  if [[ -f "$PROXY_LOG" ]]; then
    echo "tools proxy log: $PROXY_LOG" >&2
  fi
  if [[ -f "$QEMU_LOG" ]]; then
    echo "fake QEMU log: $QEMU_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${TOOLS_PID:-}" ]]; then
    kill "$TOOLS_PID" 2>/dev/null || true
    wait "$TOOLS_PID" 2>/dev/null || true
  fi
  if [[ -n "${PROXY_PID:-}" ]]; then
    kill "$PROXY_PID" 2>/dev/null || true
    wait "$PROXY_PID" 2>/dev/null || true
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
}

wait_last_result() {
  local runtime_metadata="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-runtime.json"

  for _ in {1..100}; do
    if python3 - "$runtime_metadata" "$REQUEST_ID" <<'PY'
import json
import sys

path, request_id = sys.argv[1:3]
try:
    with open(path, "r", encoding="utf-8") as handle:
        metadata = json.load(handle)
except FileNotFoundError:
    sys.exit(1)

result = metadata.get("last_command_result")
if not isinstance(result, dict):
    sys.exit(1)

checks = [
    result.get("request_id") == request_id,
    result.get("capability") == "time-sync",
    result.get("ok") is True,
    result.get("error_code") is None,
    result.get("message") is None,
]
sys.exit(0 if all(checks) else 1)
PY
    then
      return 0
    fi
    sleep 0.05
  done

  fail "timed out waiting for guest-tools time-sync result"
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for the socket proxy"

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

assert_command_fails_contains \
  "time-sync without connected guest tools" \
  "guest tools session is not connected" \
  bridgevm_socket guest-tools time-sync "$VM_NAME" \
    --unix-epoch-millis "$EXPECTED_MILLIS" \
    --request-id "time-sync-no-session"

python3 - "$GUEST_TOOLS_SOCKET" "$PROXY_SOCKET" "$PROXY_READY" "$REQUEST_ID" "$EXPECTED_MILLIS" >"$PROXY_LOG" 2>&1 <<'PY' &
import json
import os
import select
import socket
import sys

daemon_path, tools_path, ready_path, request_id, expected_millis = sys.argv[1:6]
expected_millis = int(expected_millis)

for path in (daemon_path, tools_path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

tools_server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
tools_server.bind(tools_path)
tools_server.listen(1)

with open(ready_path, "w", encoding="utf-8") as ready:
    ready.write("ready\n")

tools, _ = tools_server.accept()

daemon_server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
daemon_server.bind(daemon_path)
daemon_server.listen(1)
daemon, _ = daemon_server.accept()

daemon.setblocking(False)
tools.setblocking(False)

buffers = {daemon: b"", tools: b""}
validated_command = False
validated_result = False
peers = {daemon: tools, tools: daemon}
done = False
while peers and not done:
    readable, _, _ = select.select(list(peers.keys()), [], [], 0.25)
    for sock in readable:
        try:
            data = sock.recv(65536)
        except BlockingIOError:
            continue
        peer = peers.get(sock)
        if not data:
            if peer is not None:
                try:
                    peer.shutdown(socket.SHUT_WR)
                except OSError:
                    pass
                peers.pop(peer, None)
            peers.pop(sock, None)
            sock.close()
            continue
        if sock is daemon:
            buffers[sock] += data
            while b"\n" in buffers[sock]:
                line, buffers[sock] = buffers[sock].split(b"\n", 1)
                frame = json.loads(line.decode("utf-8"))
                message = frame.get("message", {})
                time_sync = message.get("TimeSync") if isinstance(message, dict) else None
                if time_sync is not None:
                    assert frame.get("request_id") == request_id, frame
                    assert time_sync.get("unix_epoch_millis") == expected_millis, frame
                    validated_command = True
                peer.sendall(line + b"\n")
        elif peer is not None:
            buffers[sock] += data
            while b"\n" in buffers[sock]:
                line, buffers[sock] = buffers[sock].split(b"\n", 1)
                frame = json.loads(line.decode("utf-8"))
                message = frame.get("message", {})
                result = message.get("CommandResult") if isinstance(message, dict) else None
                if result is not None and result.get("request_id") == request_id:
                    assert result.get("ok") is True, frame
                    assert result.get("error_code") is None, frame
                    assert result.get("message") is None, frame
                    validated_result = True
                    done = True
                peer.sendall(line + b"\n")

daemon_server.close()
tools_server.close()

assert validated_command, "time-sync command frame was not observed"
assert validated_result, "time-sync command result frame was not observed"
PY
PROXY_PID=$!

for _ in {1..100}; do
  if [[ -S "$PROXY_SOCKET" && -f "$PROXY_READY" ]]; then
    break
  fi
  if ! kill -0 "$PROXY_PID" 2>/dev/null; then
    fail "tools proxy exited before becoming ready"
  fi
  sleep 0.05
done
[[ -S "$PROXY_SOCKET" ]] || fail "tools proxy socket was not ready"

cargo run --quiet -p bridgevm-tools-linux -- \
  --socket "$PROXY_SOCKET" \
  --token-file "$GUEST_TOOLS_TOKEN" \
  --capability heartbeat:1 \
  --capability time-sync:1 \
  --no-guest-ip \
  --no-metrics \
  >"$TOOLS_LOG" 2>&1 &
TOOLS_PID=$!

for _ in {1..100}; do
  if [[ -S "$GUEST_TOOLS_SOCKET" ]]; then
    break
  fi
  if ! kill -0 "$PROXY_PID" 2>/dev/null; then
    fail "tools proxy exited before publishing daemon guest-tools socket"
  fi
  if ! kill -0 "$TOOLS_PID" 2>/dev/null; then
    fail "bridgevm-tools-linux exited before daemon guest-tools socket was published"
  fi
  sleep 0.05
done
[[ -S "$GUEST_TOOLS_SOCKET" ]] || fail "daemon guest-tools socket was not ready"

connected=""
for _ in {1..100}; do
  connected="$(bridgevm_socket guest-tools status "$VM_NAME" 2>/dev/null || true)"
  if [[ "$connected" == *"Runtime connected: true"* ]]; then
    break
  fi
  if ! kill -0 "$TOOLS_PID" 2>/dev/null; then
    fail "bridgevm-tools-linux exited before connecting"
  fi
  sleep 0.05
done
assert_contains "$connected" "Runtime connected: true" "guest-tools status"
assert_contains "$connected" "time-sync" "guest-tools status"

assert_command_fails_contains \
  "time-sync invalid timestamp" \
  "InvalidTimestamp" \
  bridgevm_socket guest-tools time-sync "$VM_NAME" \
    --unix-epoch-millis 0 \
    --request-id "time-sync-zero"

dispatch_output="$(bridgevm_socket guest-tools time-sync "$VM_NAME" \
  --unix-epoch-millis "$EXPECTED_MILLIS" \
  --request-id "$REQUEST_ID")"
assert_contains "$dispatch_output" "Guest tools command sent for $VM_NAME" "time-sync dispatch"
assert_contains "$dispatch_output" "Request ID: $REQUEST_ID" "time-sync dispatch"
assert_contains "$dispatch_output" "Pending commands: 1" "time-sync dispatch"

wait_last_result

if ! wait "$PROXY_PID"; then
  PROXY_PID=""
  fail "tools proxy harness failed"
fi
PROXY_PID=""

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

PRESERVE_STORE=0
echo "PASS: guest-tools time-sync live socket smoke ($STORE)"
