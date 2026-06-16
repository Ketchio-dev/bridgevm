#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bvm-aft.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="app-consistent-live"
SNAPSHOT_NAME="live-freeze-thaw"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
QEMU_LOG="$STORE/fake-qemu.log"
PROXY_SOCKET="$STORE/tools-proxy.sock"
PROXY_READY="$STORE/tools-proxy.ready"
PROXY_LOG="$STORE/tools-proxy.log"
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

python3 - "$GUEST_TOOLS_SOCKET" "$PROXY_SOCKET" "$PROXY_READY" >"$PROXY_LOG" 2>&1 <<'PY' &
import os
import select
import socket
import sys

daemon_path, tools_path, ready_path = sys.argv[1:4]

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

peers = {daemon: tools, tools: daemon}
while peers:
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
        if peer is not None:
            peer.sendall(data)

daemon_server.close()
tools_server.close()
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
  --capability guest-ip:1 \
  --capability time-sync:1 \
  --capability guest-metrics:1 \
  --capability fs-freeze:1 \
  --capability fs-thaw:1 \
  --capability clipboard:1 \
  --capability display-resize:1 \
  --capability shared-folders:1 \
  --capability applications:1 \
  --capability windows:1 \
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
assert_contains "$connected" "fs-freeze" "guest-tools status"
assert_contains "$connected" "fs-thaw" "guest-tools status"

execution="$(
  bridgevm_socket snapshot execute-application-consistent "$VM_NAME" "$SNAPSHOT_NAME"
)"
assert_contains "$execution" "Application-consistent snapshot execution for $VM_NAME" "execution"
assert_contains "$execution" "Snapshot: $SNAPSHOT_NAME" "execution"
assert_contains "$execution" "Freeze result: true" "execution"
assert_contains "$execution" "Thaw result: true" "execution"
assert_contains \
  "$execution" \
  "entered simulated filesystem freeze scaffold boundary" \
  "execution"
assert_contains \
  "$execution" \
  "left simulated filesystem thaw scaffold boundary" \
  "execution"
assert_contains \
  "$execution" \
  "no OS fsfreeze was executed and application consistency is not guaranteed" \
  "execution"
assert_contains "$execution" "Pending after freeze: 0" "execution"
assert_contains "$execution" "Pending after thaw: 0" "execution"
assert_contains "$execution" "still does not prove OS-level application consistency" "execution"

SNAPSHOT_METADATA="$STORE/vms/$VM_NAME.vmbridge/metadata/snapshots.json"
[[ -f "$SNAPSHOT_METADATA" ]] || fail "snapshot metadata missing: $SNAPSHOT_METADATA"
grep -q "\"name\": \"$SNAPSHOT_NAME\"" "$SNAPSHOT_METADATA" \
  || fail "snapshot metadata did not record $SNAPSHOT_NAME"
grep -q '"kind": "application-consistent"' "$SNAPSHOT_METADATA" \
  || fail "snapshot metadata did not record application-consistent kind"

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

kill "$TOOLS_PID" 2>/dev/null || true
wait "$TOOLS_PID" 2>/dev/null || true
TOOLS_PID=""

kill "$PROXY_PID" 2>/dev/null || true
wait "$PROXY_PID" 2>/dev/null || true
PROXY_PID=""

kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
DAEMON_PID=""

PRESERVE_STORE=0
echo "PASS: application-consistent freeze/thaw daemon live socket smoke ($STORE)"
