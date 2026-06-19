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
DESKTOP_LOG="$STORE/desktop-tools.log"
FRAMEBUFFER_RGBA=""
DAEMON_PID=""
TOOLS_PID=""
PROXY_PID=""
PRESERVE_STORE=1
REAL_BACKEND="${BRIDGEVM_APP_WINDOW_REAL_BACKEND:-0}"
REAL_USER_HOME="${HOME:-}"

unset BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE
unset BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH
unset BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT
unset BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR

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

write_app_direct_display_runner_metadata() {
  python3 - "$STORE/vms/$VM_NAME.vmbridge/metadata/runner.json" "$FRAMEBUFFER_RGBA" "$STORE/vms/$VM_NAME.vmbridge/logs/lightvm.log" "$VM_NAME" <<'PY'
import json
import os
import sys

runner_path, framebuffer_path, log_path, vm_name = sys.argv[1:5]
os.makedirs(os.path.dirname(runner_path), exist_ok=True)
os.makedirs(os.path.dirname(log_path), exist_ok=True)

metadata = {
    "engine": "lightvm",
    "pid": 4242,
    "command": [
        "lightvm-runner",
        vm_name,
        "--launch",
        "--apple-vz-display",
        "--apple-vz-display-width",
        "1440",
        "--apple-vz-display-height",
        "900",
        "--apple-vz-proxy-framebuffer-rgba-file",
        framebuffer_path,
    ],
    "log_path": log_path,
    "started_at_unix": 1,
    "dry_run": False,
}
with open(runner_path, "w", encoding="utf-8") as handle:
    json.dump(metadata, handle, sort_keys=True)
PY
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
elif payload_check == "real-applications-list":
    applications = payload.get("applications") or []
    by_id = {app.get("id"): app for app in applications if isinstance(app, dict)}
    checks.extend([
        by_id.get("org.bridgevm.real-terminal.desktop", {}).get("name") == "Real Terminal",
        by_id.get("org.bridgevm.real-terminal.desktop", {}).get("source") == "linux-desktop-file",
        by_id.get("org.bridgevm.real-files.desktop", {}).get("name") == "Real Files",
        by_id.get("org.bridgevm.real-files.desktop", {}).get("source") == "linux-desktop-file",
    ])
elif payload_check == "real-application-launch":
    application = payload.get("application") or {}
    checks.extend([
        application.get("id") == "org.bridgevm.real-terminal.desktop",
        application.get("name") == "Real Terminal",
        application.get("launched") is True,
        application.get("source") == "linux-desktop-file",
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
elif payload_check == "window-bounds":
    window = payload.get("window") or {}
    bounds = window.get("bounds") or {}
    checks.extend([
        window.get("id") == "window-1",
        window.get("title") == "BridgeVM Linux Desktop",
        window.get("bounds_changed") is True,
        bounds.get("x") == 50,
        bounds.get("y") == 60,
        bounds.get("width") == 1024,
        bounds.get("height") == 768,
    ])
elif payload_check == "window-pointer":
    window = payload.get("window") or {}
    input_payload = window.get("input") or {}
    checks.extend([
        window.get("id") == "window-1",
        window.get("title") == "BridgeVM Linux Desktop",
        window.get("focused") is True,
        input_payload.get("kind") == "pointer",
        input_payload.get("x") == 120,
        input_payload.get("y") == 240,
        input_payload.get("action") == "click",
        input_payload.get("button") == "left",
        input_payload.get("source") == "scaffold",
    ])
elif payload_check == "window-key":
    window = payload.get("window") or {}
    input_payload = window.get("input") or {}
    checks.extend([
        window.get("id") == "window-1",
        window.get("title") == "BridgeVM Linux Desktop",
        input_payload.get("kind") == "key",
        input_payload.get("key") == "Return",
        input_payload.get("action") == "tap",
        input_payload.get("source") == "scaffold",
    ])
elif payload_check == "real-windows-list":
    windows = payload.get("windows") or []
    by_id = {window.get("id"): window for window in windows if isinstance(window, dict)}
    window = by_id.get("0x01200007", {})
    bounds = window.get("bounds") or {}
    crop_summary_path = window.get("window_crop_frame_summary_path")
    checks.extend([
        window.get("title") == "Real Terminal",
        window.get("source") == "wmctrl",
        window.get("pid") == 4242,
        bounds.get("x") == 30,
        bounds.get("y") == 40,
        bounds.get("width") == 800,
        bounds.get("height") == 600,
        isinstance(crop_summary_path, str)
        and crop_summary_path.endswith("/metadata/proxy-windows/0x01200007.json"),
    ])
    if isinstance(crop_summary_path, str):
        try:
            with open(crop_summary_path, "r", encoding="utf-8") as handle:
                crop_summary = json.load(handle)
            crop_frame = crop_summary.get("window_crop_frame") or {}
            crop_output_path = crop_frame.get("output_path")
            checks.extend([
                crop_summary.get("window_region", {}).get("window_id") == "0x01200007",
                isinstance(crop_frame.get("source_path"), str)
                and crop_frame.get("source_path").endswith(
                    "/metadata/apple-vz-display-framebuffer.rgba"
                ),
                crop_frame.get("pixel_format") == "rgba8",
                crop_frame.get("framebuffer_width") == 1440,
                crop_frame.get("framebuffer_height") == 900,
                crop_frame.get("crop_rect") == {
                    "x": 30,
                    "y": 40,
                    "width": 800,
                    "height": 600,
                },
                isinstance(crop_output_path, str),
                crop_frame.get("expected_input_bytes") == 1440 * 900 * 4,
                crop_frame.get("source_len_bytes") == 1440 * 900 * 4,
                isinstance(crop_frame.get("source_modified_unix_nanos"), int)
                and crop_frame.get("source_modified_unix_nanos") > 0,
                isinstance(crop_frame.get("refreshed_at_unix_nanos"), int)
                and crop_frame.get("refreshed_at_unix_nanos") > 0,
            ])
            if isinstance(crop_output_path, str):
                with open(crop_output_path, "rb") as handle:
                    crop_bytes = handle.read()
                checks.extend([
                    len(crop_bytes) == 800 * 600 * 4,
                    crop_bytes[:4] == bytes([0x10, 0x20, 0x30, 0xFF]),
                ])
        except (OSError, json.JSONDecodeError):
            checks.append(False)
elif payload_check == "real-window-focus":
    window = payload.get("window") or {}
    bounds = window.get("bounds") or {}
    checks.extend([
        window.get("id") == "0x01200007",
        window.get("title") == "Real Terminal",
        window.get("focused") is True,
        window.get("source") == "wmctrl",
        window.get("pid") == 4242,
        bounds.get("width") == 800,
        bounds.get("height") == 600,
    ])
elif payload_check == "real-window-bounds":
    window = payload.get("window") or {}
    bounds = window.get("bounds") or {}
    checks.extend([
        window.get("id") == "0x01200007",
        window.get("title") == "Real Terminal",
        window.get("source") == "wmctrl",
        window.get("bounds_changed") is True,
        bounds.get("x") == 50,
        bounds.get("y") == 60,
        bounds.get("width") == 1024,
        bounds.get("height") == 768,
    ])
elif payload_check == "real-window-close":
    window = payload.get("window") or {}
    bounds = window.get("bounds") or {}
    checks.extend([
        window.get("id") == "0x01200007",
        window.get("title") == "Real Terminal",
        window.get("closed") is True,
        window.get("source") == "wmctrl",
        window.get("pid") == 4242,
        bounds.get("width") == 800,
        bounds.get("height") == 600,
    ])
elif payload_check == "real-window-pointer":
    window = payload.get("window") or {}
    bounds = window.get("bounds") or {}
    input_payload = window.get("input") or {}
    checks.extend([
        window.get("id") == "0x01200007",
        window.get("title") == "Real Terminal",
        window.get("source") == "wmctrl",
        bounds.get("width") == 800,
        bounds.get("height") == 600,
        input_payload.get("kind") == "pointer",
        input_payload.get("x") == 120,
        input_payload.get("y") == 240,
        input_payload.get("action") == "click",
        input_payload.get("button") == "left",
        input_payload.get("source") == "xdotool",
    ])
elif payload_check == "real-window-key":
    window = payload.get("window") or {}
    input_payload = window.get("input") or {}
    checks.extend([
        window.get("id") == "0x01200007",
        window.get("title") == "Real Terminal",
        window.get("source") == "wmctrl",
        input_payload.get("kind") == "key",
        input_payload.get("key") == "Return",
        input_payload.get("action") == "tap",
        input_payload.get("source") == "xdotool",
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

assert_real_window_displayd_contract() {
  local plan_json="$STORE/displayd-real-window-region.json"
  cargo run --quiet -p displayd -- \
    --print-plan \
    --visibility foreground \
    --dirty-regions 4 \
    --framebuffer-width 1440 \
    --framebuffer-height 900 \
    --scale 2 \
    --window-id 0x01200007 \
    --window-title "Real Terminal" \
    --window-x 30 \
    --window-y 40 \
    --window-width 800 \
    --window-height 600 \
    --window-host-width 400 \
    --window-host-height 300 \
    >"$plan_json"

  python3 - "$plan_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    plan = json.load(handle)

expected = {
    "window_id": "0x01200007",
    "title": "Real Terminal",
    "source_rect": {"x": 30, "y": 40, "width": 800, "height": 600},
    "clipped_rect": {"x": 30, "y": 40, "width": 800, "height": 600},
    "host_size": {"width": 400, "height": 300},
    "backing_rect": {"x": 60, "y": 80, "width": 1600, "height": 1200},
    "input_mapping": {
        "coordinate_origin": "guest-framebuffer-top-left",
        "host_width": 400,
        "host_height": 300,
        "guest_x": 30,
        "guest_y": 40,
        "guest_width": 800,
        "guest_height": 600,
        "scale_x_numerator": 800,
        "scale_x_denominator": 400,
        "scale_y_numerator": 600,
        "scale_y_denominator": 300,
    },
    "presentation": "proxy-window-crop",
}

if plan.get("window_region") != expected:
    raise SystemExit(
        "displayd real-window region contract mismatch: "
        + json.dumps(plan.get("window_region"), sort_keys=True)
    )
PY
}

assert_real_window_crop_refresh() {
  local crop_rgba="$STORE/vms/$VM_NAME.vmbridge/metadata/proxy-windows/0x01200007.rgba"
  [[ -f "$crop_rgba" ]] || fail "proxy crop RGBA artifact was not created: $crop_rgba"

  python3 - "$FRAMEBUFFER_RGBA" <<'PY'
import sys

path = sys.argv[1]
with open(path, "wb") as handle:
    handle.write(bytes([0xAA, 0xBB, 0xCC, 0xFF]) * (1440 * 900))
PY

  for _ in {1..100}; do
    if python3 - "$crop_rgba" <<'PY'
import sys

with open(sys.argv[1], "rb") as handle:
    data = handle.read(4)
raise SystemExit(0 if data == bytes([0xAA, 0xBB, 0xCC, 0xFF]) else 1)
PY
    then
      return 0
    fi
    sleep 0.05
  done

  fail "proxy crop RGBA artifact did not refresh after framebuffer rewrite"
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

if [[ "$REAL_BACKEND" == "1" ]]; then
  mkdir -p "$STORE/home/.local/share/applications"
  cat >"$STORE/home/.local/share/applications/org.bridgevm.real-terminal.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Real Terminal
Exec=real-terminal
DESKTOP
  cat >"$STORE/home/.local/share/applications/org.bridgevm.real-files.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Real Files
Exec=real-files
DESKTOP
  cat >"$STORE/home/.local/share/applications/org.bridgevm.hidden.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Hidden App
NoDisplay=true
Exec=hidden
DESKTOP

  cat >"$FAKE_BIN/gio" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "gio $*" >>"${BRIDGEVM_DESKTOP_TOOLS_LOG:?}"
[[ "${1:-}" == "launch" ]] || exit 2
[[ -f "${2:-}" ]] || exit 3
exit 0
SH
  chmod +x "$FAKE_BIN/gio"

  cat >"$FAKE_BIN/wmctrl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "wmctrl $*" >>"${BRIDGEVM_DESKTOP_TOOLS_LOG:?}"
case "${1:-}" in
  -l)
    if [[ "${2:-}" == "-p" && "${3:-}" == "-G" ]]; then
      printf '0x01200007  0 4242 30 40 800 600 bridgevm Real Terminal\n'
    else
      printf '0x01200007  0 bridgevm Real Terminal\n'
    fi
    ;;
  -ia|-ic)
    [[ "${2:-}" == "0x01200007" ]] || exit 4
    ;;
  -ir)
    [[ "${2:-}" == "0x01200007" ]] || exit 4
    [[ "${3:-}" == "-e" ]] || exit 5
    [[ "${4:-}" == "0,50,60,1024,768" ]] || exit 6
    ;;
  *)
    exit 2
    ;;
esac
SH
  chmod +x "$FAKE_BIN/wmctrl"

  cat >"$FAKE_BIN/xdotool" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "xdotool $*" >>"${BRIDGEVM_DESKTOP_TOOLS_LOG:?}"
case "${1:-}" in
  mousemove)
    [[ "${2:-}" == "--sync" ]] || exit 5
    [[ "${3:-}" =~ ^-?[0-9]+$ ]] || exit 6
    [[ "${4:-}" =~ ^-?[0-9]+$ ]] || exit 7
    ;;
  mousedown|mouseup|click)
    [[ "${2:-}" =~ ^[123]$ ]] || exit 8
    ;;
  keydown|keyup|key)
    [[ -n "${2:-}" ]] || exit 9
    ;;
  *)
    exit 2
    ;;
esac
SH
  chmod +x "$FAKE_BIN/xdotool"
fi

PATH="$FAKE_BIN:$PATH" bridgevm create "$VM_NAME" \
  --os ubuntu \
  --arch x86_64 \
  --mode compatibility >/dev/null
PATH="$FAKE_BIN:$PATH" bridgevm disk create "$VM_NAME" >/dev/null

if [[ "$REAL_BACKEND" == "1" ]]; then
  FRAMEBUFFER_RGBA="$STORE/vms/$VM_NAME.vmbridge/metadata/apple-vz-display-framebuffer.rgba"
  mkdir -p "$(dirname "$FRAMEBUFFER_RGBA")"
  python3 - "$FRAMEBUFFER_RGBA" <<'PY'
import sys

path = sys.argv[1]
with open(path, "wb") as handle:
    handle.write(bytes([0x10, 0x20, 0x30, 0xFF]) * (1440 * 900))
PY
fi

if [[ "$REAL_BACKEND" == "1" ]]; then
  PATH="$FAKE_BIN:$PATH" \
    BRIDGEVM_FAKE_QEMU_LOG="$QEMU_LOG" \
    BRIDGEVM_PROXY_WINDOW_BACKING_SCALE=2 \
    cargo run --quiet -p bridgevm-daemon -- \
      --store "$STORE" \
      --reconcile-interval-ms 25 \
      >"$DAEMON_LOG" 2>&1 &
else
  PATH="$FAKE_BIN:$PATH" BRIDGEVM_FAKE_QEMU_LOG="$QEMU_LOG" \
    cargo run --quiet -p bridgevm-daemon -- \
      --store "$STORE" \
      --reconcile-interval-ms 25 \
      >"$DAEMON_LOG" 2>&1 &
fi
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
if [[ "$REAL_BACKEND" == "1" ]]; then
  write_app_direct_display_runner_metadata
fi

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

if [[ "$REAL_BACKEND" == "1" ]]; then
  HOME="$STORE/home" \
    RUSTUP_HOME="${RUSTUP_HOME:-$REAL_USER_HOME/.rustup}" \
    CARGO_HOME="${CARGO_HOME:-$REAL_USER_HOME/.cargo}" \
    DISPLAY=":99" \
    PATH="$FAKE_BIN:$PATH" \
    BRIDGEVM_DESKTOP_TOOLS_LOG="$DESKTOP_LOG" \
    cargo run --quiet -p bridgevm-tools-linux -- \
      --socket "$PROXY_SOCKET" \
      --token-file "$GUEST_TOOLS_TOKEN" \
      --capability heartbeat:1 \
      --capability applications:1 \
      --capability windows:1 \
      --no-guest-ip \
      --no-metrics \
      >"$TOOLS_LOG" 2>&1 &
else
  cargo run --quiet -p bridgevm-tools-linux -- \
    --socket "$PROXY_SOCKET" \
    --token-file "$GUEST_TOOLS_TOKEN" \
    --capability heartbeat:1 \
    --capability applications:1 \
    --capability windows:1 \
    --no-guest-ip \
    --no-metrics \
    >"$TOOLS_LOG" 2>&1 &
fi
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

if [[ "$REAL_BACKEND" == "1" ]]; then
  dispatch_guest_tools "apps-list-1" list-applications
  wait_last_result \
    "apps-list-1" \
    "applications" \
    "true" \
    "applications: org.bridgevm.real-files.desktop:Real Files,org.bridgevm.real-terminal.desktop:Real Terminal" \
    "" \
    "real-applications-list"

  dispatch_guest_tools "app-launch-1" launch-application --id org.bridgevm.real-terminal.desktop
  wait_last_result \
    "app-launch-1" \
    "applications" \
    "true" \
    "launched application Real Terminal" \
    "" \
    "real-application-launch"

  grep -Fq "gio launch $STORE/home/.local/share/applications/org.bridgevm.real-terminal.desktop" "$DESKTOP_LOG" \
    || fail "real backend launch did not invoke fake gio; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
else
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
fi

dispatch_guest_tools "app-launch-missing" launch-application --id org.bridgevm.missing
wait_last_result \
  "app-launch-missing" \
  "applications" \
  "false" \
  "application org.bridgevm.missing was not found" \
  "application-not-found"

if [[ "$REAL_BACKEND" == "1" ]]; then
  dispatch_guest_tools "windows-list-1" list-windows
  wait_last_result \
    "windows-list-1" \
    "windows" \
    "true" \
    "windows: 0x01200007:Real Terminal" \
    "" \
    "real-windows-list"
  assert_real_window_displayd_contract
  assert_real_window_crop_refresh

  dispatch_guest_tools "window-focus-1" focus-window --id 0x01200007
  wait_last_result \
    "window-focus-1" \
    "windows" \
    "true" \
    "focused window Real Terminal" \
    "" \
    "real-window-focus"

  dispatch_guest_tools "window-bounds-1" set-window-bounds --id 0x01200007 --x 50 --y 60 --width 1024 --height 768
  wait_last_result \
    "window-bounds-1" \
    "windows" \
    "true" \
    "set bounds for window Real Terminal" \
    "" \
    "real-window-bounds"

  dispatch_guest_tools "window-pointer-1" window-pointer --id 0x01200007 --x 120 --y 240 --action click --button left
  wait_last_result \
    "window-pointer-1" \
    "windows" \
    "true" \
    "sent pointer input to window Real Terminal" \
    "" \
    "real-window-pointer"

  dispatch_guest_tools "window-key-1" window-key --id 0x01200007 --key Return --action tap
  wait_last_result \
    "window-key-1" \
    "windows" \
    "true" \
    "sent key input to window Real Terminal" \
    "" \
    "real-window-key"

  dispatch_guest_tools "window-close-1" close-window --id 0x01200007
  wait_last_result \
    "window-close-1" \
    "windows" \
    "true" \
    "closed window Real Terminal" \
    "" \
    "real-window-close"

  grep -Fq "wmctrl -ia 0x01200007" "$DESKTOP_LOG" \
    || fail "real backend focus did not invoke fake wmctrl; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
  grep -Fq "wmctrl -ir 0x01200007 -e 0,50,60,1024,768" "$DESKTOP_LOG" \
    || fail "real backend bounds sync did not invoke fake wmctrl -ir; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
  grep -Fq "wmctrl -ic 0x01200007" "$DESKTOP_LOG" \
    || fail "real backend close did not invoke fake wmctrl; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
  grep -Fq "xdotool mousemove --sync 120 240" "$DESKTOP_LOG" \
    || fail "real backend pointer input did not invoke fake xdotool mousemove; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
  grep -Fq "xdotool click 1" "$DESKTOP_LOG" \
    || fail "real backend pointer input did not invoke fake xdotool click; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"
  grep -Fq "xdotool key Return" "$DESKTOP_LOG" \
    || fail "real backend key input did not invoke fake xdotool; log: $(cat "$DESKTOP_LOG" 2>/dev/null || true)"

  dispatch_guest_tools "window-focus-missing" focus-window --id 0x99999999
  wait_last_result \
    "window-focus-missing" \
    "windows" \
    "false" \
    "window 0x99999999 was not found" \
    "window-not-found"
else
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

  dispatch_guest_tools "window-bounds-1" set-window-bounds --id window-1 --x 50 --y 60 --width 1024 --height 768
  wait_last_result \
    "window-bounds-1" \
    "windows" \
    "true" \
    "set bounds for window BridgeVM Linux Desktop" \
    "" \
    "window-bounds"

  dispatch_guest_tools "window-pointer-1" window-pointer --id window-1 --x 120 --y 240 --action click --button left
  wait_last_result \
    "window-pointer-1" \
    "windows" \
    "true" \
    "accepted pointer input for window BridgeVM Linux Desktop" \
    "" \
    "window-pointer"

  dispatch_guest_tools "window-key-1" window-key --id window-1 --key Return --action tap
  wait_last_result \
    "window-key-1" \
    "windows" \
    "true" \
    "accepted key input for window BridgeVM Linux Desktop" \
    "" \
    "window-key"

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
fi

PATH="$FAKE_BIN:$PATH" bridgevm_socket stop "$VM_NAME" >/dev/null

PRESERVE_STORE=0
if [[ "$REAL_BACKEND" == "1" ]]; then
  echo "PASS: guest-tools application/window real desktop backend socket smoke ($STORE)"
else
  echo "PASS: guest-tools application/window live socket smoke ($STORE)"
fi
