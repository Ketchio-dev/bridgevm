#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-app-window.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="app-window-live"
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

wait_last_result() {
  local request_id="$1"
  local capability="$2"
  local ok="$3"
  local message="$4"
  local error_code="${5:-}"
  local payload_check="${6:-}"
  local runtime_metadata="$STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools-runtime.json"

  for _ in {1..100}; do
    if python3 - "$runtime_metadata" "$request_id" "$capability" "$ok" "$message" "$error_code" "$payload_check" <<'PY'
import json
import sys

path, request_id, capability, ok_text, message, error_code, payload_check = sys.argv[1:8]
try:
    with open(path, "r", encoding="utf-8") as handle:
        metadata = json.load(handle)
except FileNotFoundError:
    sys.exit(1)

result = metadata.get("last_command_result")
if not isinstance(result, dict):
    sys.exit(1)

expected_ok = ok_text == "true"
checks = [
    result.get("request_id") == request_id,
    result.get("capability") == capability,
    result.get("ok") is expected_ok,
    result.get("message") == message,
]
if error_code:
    checks.append(result.get("error_code") == error_code)
else:
    checks.append(result.get("error_code") is None)

payload = result.get("result") or {}
if payload_check == "applications-list":
    applications = payload.get("applications") or []
    by_id = {app.get("id"): app for app in applications if isinstance(app, dict)}
    checks.extend([
        by_id.get("org.bridgevm.files", {}).get("name") == "Files",
        by_id.get("org.bridgevm.terminal", {}).get("name") == "Terminal",
    ])
elif payload_check == "application-launch":
    application = payload.get("application") or {}
    checks.extend([
        application.get("id") == "org.bridgevm.terminal",
        application.get("name") == "Terminal",
        application.get("launched") is True,
    ])
elif payload_check == "windows-list":
    windows = payload.get("windows") or []
    by_id = {window.get("id"): window for window in windows if isinstance(window, dict)}
    checks.extend([
        by_id.get("window-1", {}).get("title") == "BridgeVM Linux Desktop",
        by_id.get("window-1", {}).get("focused") is True,
    ])
elif payload_check == "window-focus":
    window = payload.get("window") or {}
    checks.extend([
        window.get("id") == "window-1",
        window.get("title") == "BridgeVM Linux Desktop",
        window.get("focused") is True,
    ])
elif payload_check == "window-close":
    window = payload.get("window") or {}
    checks.extend([
        window.get("id") == "window-1",
        window.get("title") == "BridgeVM Linux Desktop",
        window.get("closed") is True,
    ])
elif payload_check:
    raise SystemExit(f"unknown payload check: {payload_check}")

sys.exit(0 if all(checks) else 1)
PY
    then
      return 0
    fi
    sleep 0.05
  done

  fail "timed out waiting for guest-tools result $request_id"
}

dispatch_guest_tools() {
  local request_id="$1"
  local subcommand="$2"
  shift
  shift
  local output
  output="$(bridgevm_socket guest-tools "$subcommand" "$VM_NAME" "$@" --request-id "$request_id")"
  assert_contains "$output" "Guest tools command sent for $VM_NAME" "$request_id dispatch"
  assert_contains "$output" "Request ID: $request_id" "$request_id dispatch"
  assert_contains "$output" "Pending commands:" "$request_id dispatch"
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
assert_contains "$connected" "applications" "guest-tools status"
assert_contains "$connected" "windows" "guest-tools status"

dispatch_guest_tools "apps-list-1" list-applications
wait_last_result \
  "apps-list-1" \
  "applications" \
  "true" \
  "applications: org.bridgevm.files:Files,org.bridgevm.terminal:Terminal" \
  "" \
  "applications-list"

dispatch_guest_tools "app-launch-1" launch-application --id org.bridgevm.terminal
wait_last_result \
  "app-launch-1" \
  "applications" \
  "true" \
  "accepted launch request for application Terminal" \
  "" \
  "application-launch"

dispatch_guest_tools "app-launch-missing" launch-application --id org.bridgevm.missing
wait_last_result \
  "app-launch-missing" \
  "applications" \
  "false" \
  "application org.bridgevm.missing was not found" \
  "application-not-found"

dispatch_guest_tools "windows-list-1" list-windows
wait_last_result \
  "windows-list-1" \
  "windows" \
  "true" \
  "windows: window-1:BridgeVM Linux Desktop" \
  "" \
  "windows-list"

dispatch_guest_tools "window-focus-1" focus-window --id window-1
wait_last_result \
  "window-focus-1" \
  "windows" \
  "true" \
  "accepted focus request for window BridgeVM Linux Desktop" \
  "" \
  "window-focus"

dispatch_guest_tools "window-close-1" close-window --id window-1
wait_last_result \
  "window-close-1" \
  "windows" \
  "true" \
  "closed window BridgeVM Linux Desktop" \
  "" \
  "window-close"

dispatch_guest_tools "window-focus-closed" focus-window --id window-1
wait_last_result \
  "window-focus-closed" \
  "windows" \
  "false" \
  "window window-1 was not found" \
  "window-not-found"

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

PRESERVE_STORE=0
echo "PASS: guest-tools application/window live socket smoke ($STORE)"
