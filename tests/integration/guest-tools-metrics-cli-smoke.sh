#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bvm-guest-metrics.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="guest-tools-metrics"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
TOOLS_LOG="$STORE/bridgevm-tools-linux.log"
QEMU_LOG="$STORE/fake-qemu.log"
PROXY_SOCKET="$STORE/tools-proxy.sock"
PROXY_READY="$STORE/tools-proxy.ready"
PROXY_LOG="$STORE/tools-proxy.log"
METRICS_SENT="$STORE/metrics-sent"
CPU_PERCENT=42
MEMORY_USED_MIB=1536
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

policy_status="$(bridgevm guest-tools status "$VM_NAME")"
assert_contains "$policy_status" "Capability: guest-metrics" "guest-tools policy"
assert_contains "$policy_status" "Enabled by: diagnostics" "guest-tools policy"
assert_contains "$policy_status" "Runtime connected: false" "guest-tools policy"

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
  "$PROXY_SOCKET" \
  "$PROXY_READY" \
  "$METRICS_SENT" \
  "$CPU_PERCENT" \
  "$MEMORY_USED_MIB" \
  >"$PROXY_LOG" 2>&1 <<'PY' &
import json
import os
import select
import socket
import sys
import time

daemon_path, tools_path, ready_path, metrics_sent_path, expected_cpu, expected_memory = sys.argv[1:7]
expected_cpu = int(expected_cpu)
expected_memory = int(expected_memory)

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
peers = {daemon: tools, tools: daemon}
validated_metrics = False
deadline = time.monotonic() + 10

while peers and time.monotonic() < deadline:
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
        buffers[sock] += data
        while b"\n" in buffers[sock]:
            line, buffers[sock] = buffers[sock].split(b"\n", 1)
            if sock is tools:
                frame = json.loads(line.decode("utf-8"))
                message = frame.get("message", {})
                metrics = message.get("GuestMetrics") if isinstance(message, dict) else None
                if metrics is not None:
                    assert metrics.get("cpu_percent") == expected_cpu, frame
                    assert metrics.get("memory_used_mib") == expected_memory, frame
                    validated_metrics = True
                    with open(metrics_sent_path, "w", encoding="utf-8") as metrics_sent:
                        metrics_sent.write("sent\n")
            if peer is not None:
                peer.sendall(line + b"\n")
    if validated_metrics and os.path.exists(metrics_sent_path):
        time.sleep(0.5)
        break

daemon_server.close()
tools_server.close()

assert validated_metrics, "GuestMetrics frame was not observed"
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
  --capability guest-metrics:1 \
  --no-guest-ip \
  --metrics-cpu-percent "$CPU_PERCENT" \
  --metrics-memory-used-mib "$MEMORY_USED_MIB" \
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

for _ in {1..100}; do
  if [[ -f "$METRICS_SENT" ]]; then
    break
  fi
  if ! kill -0 "$PROXY_PID" 2>/dev/null; then
    fail "tools proxy exited before observing GuestMetrics"
  fi
  if ! kill -0 "$TOOLS_PID" 2>/dev/null; then
    fail "bridgevm-tools-linux exited before publishing GuestMetrics"
  fi
  sleep 0.05
done
[[ -f "$METRICS_SENT" ]] || fail "GuestMetrics frame was not sent"

runtime_metadata="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-runtime.json"
for _ in {1..100}; do
  if python3 - "$runtime_metadata" "$CPU_PERCENT" "$MEMORY_USED_MIB" <<'PY'
import json
import sys

path, expected_cpu, expected_memory = sys.argv[1:4]
expected_cpu = int(expected_cpu)
expected_memory = int(expected_memory)
try:
    with open(path, "r", encoding="utf-8") as handle:
        runtime = json.load(handle)
except FileNotFoundError:
    sys.exit(1)

metrics = runtime.get("metrics")
if not isinstance(metrics, dict):
    sys.exit(1)

checks = [
    runtime.get("connected") is True,
    runtime.get("guest_os") == "linux",
    "guest-metrics" in runtime.get("capabilities", []),
    metrics.get("cpu_percent") == expected_cpu,
    metrics.get("memory_used_mib") == expected_memory,
    isinstance(metrics.get("updated_at_unix"), int),
]
sys.exit(0 if all(checks) else 1)
PY
  then
    break
  fi
  if ! kill -0 "$TOOLS_PID" 2>/dev/null; then
    fail "bridgevm-tools-linux exited before metrics metadata was recorded"
  fi
  sleep 0.05
done

python3 - "$runtime_metadata" "$CPU_PERCENT" "$MEMORY_USED_MIB" <<'PY' \
  || fail "GuestMetrics runtime metadata was not recorded with the expected fields"
import json
import sys

path, expected_cpu, expected_memory = sys.argv[1:4]
expected_cpu = int(expected_cpu)
expected_memory = int(expected_memory)
with open(path, "r", encoding="utf-8") as handle:
    runtime = json.load(handle)

metrics = runtime.get("metrics") or {}
assert runtime.get("connected") is True, runtime
assert runtime.get("guest_os") == "linux", runtime
assert "guest-metrics" in runtime.get("capabilities", []), runtime
assert metrics.get("cpu_percent") == expected_cpu, metrics
assert metrics.get("memory_used_mib") == expected_memory, metrics
assert isinstance(metrics.get("updated_at_unix"), int), metrics
PY

status_output="$(bridgevm_socket guest-tools status "$VM_NAME")"
assert_contains "$status_output" "Runtime connected: true" "guest-tools status"
assert_contains "$status_output" "Capability: guest-metrics" "guest-tools status"
assert_contains "$status_output" "Guest CPU percent: $CPU_PERCENT" "guest-tools status"
assert_contains "$status_output" "Guest memory used MiB: $MEMORY_USED_MIB" "guest-tools status"
assert_contains "$status_output" "Guest metrics updated:" "guest-tools status"

if ! wait "$PROXY_PID"; then
  PROXY_PID=""
  fail "tools proxy harness failed"
fi
PROXY_PID=""

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

PRESERVE_STORE=0
echo "PASS: guest-tools metrics live socket smoke ($STORE)"
