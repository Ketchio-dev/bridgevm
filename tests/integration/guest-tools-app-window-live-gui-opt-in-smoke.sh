#!/usr/bin/env bash
set -euo pipefail

# guest-tools-app-window-live-gui-opt-in-smoke.sh
#
# Proves the Coherence-lite Linux application/window backend crosses into real
# desktop tools inside a booted guest. This is still not host-window Coherence:
# the guest runs an Xvfb + openbox desktop, the agent launches a real .desktop
# app through gio, and window list/focus/close go through wmctrl over the live
# virtio-serial guest-tools transport. The harness also preserves a host-side
# crop artifact generated from the real wmctrl bounds and a synthetic RGBA
# framebuffer, proving the live guest-window metadata can drive displayd's
# proxy-window crop primitive without claiming true framebuffer streaming.
#
# OPT-IN (heavy -- boots a real VM and apt-installs X11 desktop tools). SKIPS unless:
#   BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1
#   BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK=<path to a bootable arm64 Linux cloud qcow2>
# The guest needs working apt + network (the NoCloud NAT default is fine).

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() { echo "SKIP: $*"; exit 0; }

[[ "${BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 to run the app/window live GUI smoke"
[[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK to a bootable arm64 Linux cloud qcow2 (cloud-init enabled)"
[[ -f "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK does not exist: $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK"
command -v qemu-system-aarch64 >/dev/null 2>&1 || skip "qemu-system-aarch64 must be available"
command -v hdiutil >/dev/null 2>&1 || skip "hdiutil (macOS) is required for the cidata seed ISO"
command -v python3 >/dev/null 2>&1 || skip "python3 is required to drive the guest-tools socket"
if [[ ! -x "$ROOT/target/release/displayd" ]]; then
  command -v cargo >/dev/null 2>&1 || \
    skip "cargo or target/release/displayd is required to materialize the live window crop proof"
fi
qemu-system-aarch64 -accel help 2>/dev/null | grep -q '\bhvf\b' || \
  skip "qemu-system-aarch64 must support the hvf accelerator (Apple Silicon)"

AGENT_BINARY="${BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY:-$ROOT/target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux}"
[[ -f "$AGENT_BINARY" ]] || \
  skip "cross-compiled agent not found at $AGENT_BINARY (build: scripts/build-guest-agent-linux.sh)"
STALE_AGENT_SOURCE="$(
  find \
    "$ROOT/runners/bridgevm-tools-linux" \
    "$ROOT/crates/bridgevm-agent-protocol" \
    "$ROOT/crates/bridgevm-agentd" \
    -type f -name '*.rs' -newer "$AGENT_BINARY" -print -quit
)"
[[ -z "$STALE_AGENT_SOURCE" ]] || \
  skip "cross-compiled agent is older than $STALE_AGENT_SOURCE (rebuild: scripts/build-guest-agent-linux.sh)"

TIMEOUT_SECONDS="${BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS:-480}"
[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]] || skip "timeout must be a positive integer"

CREATED_STORE=0
if [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_STORE:-}" ]]; then
  STORE="$BRIDGEVM_LIVE_GUEST_TOOLS_STORE"
else
  STORE="$(mktemp -d "/tmp/bvm-aw.XXXXXX")"   # short prefix: 104-byte AF_UNIX cap
  CREATED_STORE=1
fi
VM_NAME="${BRIDGEVM_LIVE_GUEST_TOOLS_VM:-gt-aw}"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
WORK="$(mktemp -d "/tmp/bvm-aw-work.XXXXXX")"
SEED_DIR="$WORK/seed"
SEED_ISO="$WORK/cidata-seed.iso"
SESSION_LOG="$WORK/session.log"
SESSION_ERR="$WORK/session.err"
WINDOW_PAYLOAD_JSON="$WORK/live-window-payload.json"
CROP_REQUEST_JSON="$WORK/live-window-crop-request.json"
CROP_FRAMEBUFFER_RGBA="$WORK/live-window-framebuffer.rgba"
CROP_SUMMARY_JSON="$WORK/live-window-crop.json"
CROP_RGBA="$WORK/live-window-crop.rgba"
CROP_PROOF_JSON="$WORK/live-window-proxy-crop-proof.json"
DEVICE="/dev/virtio-ports/org.bridgevm.guest-tools.0"

DESKTOP_READY_MARKER="BRIDGEVM-DESKTOP-READY"
APP_LAUNCH_MARKER="BRIDGEVM-APP-LAUNCHED"
WINDOW_TITLE="BridgeVM Live Terminal"
APP_ID="org.bridgevm.live-terminal.desktop"

if [[ -x "$ROOT/target/release/bridgevm" ]]; then
  bridgevm() { "$ROOT/target/release/bridgevm" --store "$STORE" "$@"; }
else
  bridgevm() { cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"; }
fi

displayd() {
  if [[ -x "$ROOT/target/release/displayd" ]]; then
    "$ROOT/target/release/displayd" "$@"
  else
    cargo run --quiet -p displayd -- "$@"
  fi
}

QEMU_PID=""
cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$QEMU_PID" 2>/dev/null || break; sleep 0.5; done
    kill -9 "$QEMU_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  if [[ -f "$BUNDLE/logs/serial.log" ]]; then
    echo "--- last serial log lines (printable) ---" >&2
    tail -c 12000 "$BUNDLE/logs/serial.log" | tr -cd '[:print:]\n' | tail -80 >&2 || true
  fi
  [[ -s "$SESSION_ERR" ]] && { echo "--- session stderr ---" >&2; cat "$SESSION_ERR" >&2; }
  [[ -s "$SESSION_LOG" ]] && { echo "--- session stdout ---" >&2; cat "$SESSION_LOG" >&2; }
  echo "Store preserved at: $STORE" >&2
  echo "Work dir preserved at: $WORK" >&2
  exit 1
}

if [[ ! -d "$BUNDLE" ]]; then
  bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode compatibility >/dev/null \
    || fail "bridgevm create failed"
fi
mkdir -p "$BUNDLE/disks"
cp "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" "$BUNDLE/disks/root.qcow2" \
  || fail "failed to stage root.qcow2"

TOKEN_FILE="$BUNDLE/metadata/guest-tools-token.json"
[[ -f "$TOKEN_FILE" ]] || fail "guest-tools token file missing: $TOKEN_FILE"
TOKEN="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["token"])' "$TOKEN_FILE")"
[[ -n "$TOKEN" ]] || fail "empty guest-tools token"

mkdir -p "$SEED_DIR"
printf 'instance-id: bridgevm-%s\nlocal-hostname: %s\n' "$VM_NAME" "$VM_NAME" > "$SEED_DIR/meta-data"
gzip -c "$AGENT_BINARY" | base64 > "$SEED_DIR/agent.gz.b64" || fail "failed to gzip+base64 the agent"

python3 - "$SEED_DIR" "$TOKEN" "$DEVICE" "$DESKTOP_READY_MARKER" "$APP_LAUNCH_MARKER" "$WINDOW_TITLE" "$APP_ID" \
  > "$SEED_DIR/user-data" <<'PY'
import sys
seed, token, dev, ready_marker, launch_marker, window_title, app_id = sys.argv[1:8]
with open(f"{seed}/agent.gz.b64") as fh:
    blob = fh.read().strip()
indented = "\n".join("        " + line for line in blob.splitlines())
caps = " ".join(f"--capability {c}" for c in ["heartbeat", "applications", "windows"])
print(f"""#cloud-config
write_files:
  - path: /usr/local/bin/bridgevm-tools-linux.gz.b64
    permissions: '0644'
    encoding: text/plain
    content: |
{indented}
  - path: /usr/local/bin/bvm-open-live-terminal.sh
    permissions: '0755'
    content: |
      #!/bin/sh
      export DISPLAY=:99
      echo "{launch_marker}" > /dev/console 2>&1
      xterm -T "{window_title}" -e sh -c 'echo bridgevm-live-window-ready; sleep 300' &
  - path: /root/.local/share/applications/{app_id}
    permissions: '0644'
    content: |
      [Desktop Entry]
      Type=Application
      Name=BridgeVM Live Terminal
      Exec=/usr/local/bin/bvm-open-live-terminal.sh
      Terminal=false
      NoDisplay=false
  - path: /root/.local/share/applications/org.bridgevm.hidden.desktop
    permissions: '0644'
    content: |
      [Desktop Entry]
      Type=Application
      Name=BridgeVM Hidden App
      Exec=/bin/true
      NoDisplay=true
  - path: /usr/local/bin/run-bridgevm-agent.sh
    permissions: '0755'
    content: |
      #!/bin/sh
      set -x
      CONSOLE=/dev/console
      log() {{ echo "BRIDGEVM-SMOKE: $1" > "$CONSOLE" 2>&1; echo "BRIDGEVM-SMOKE: $1"; }}
      log "decode-agent-start"
      base64 -d /usr/local/bin/bridgevm-tools-linux.gz.b64 | gunzip > /usr/local/bin/bridgevm-tools-linux
      chmod 0755 /usr/local/bin/bridgevm-tools-linux
      log "agent-decoded"
      if [ ! -e "{dev}" ]; then log "DEVICE-MISSING {dev}"; exit 1; fi
      log "apt-install-start"
      export DEBIAN_FRONTEND=noninteractive
      apt-get update > "$CONSOLE" 2>&1 || log "apt-update-failed"
      apt-get install -y --no-install-recommends xvfb openbox xterm wmctrl libglib2.0-bin x11-utils > "$CONSOLE" 2>&1 \\
        || {{ log "APT-INSTALL-FAILED"; exit 1; }}
      log "apt-install-done"
      mkdir -p /tmp/.X11-unix && chmod 1777 /tmp/.X11-unix
      Xvfb :99 -screen 0 1280x1024x24 > /var/log/xvfb.log 2>&1 &
      for _ in $(seq 1 40); do
        if [ -S /tmp/.X11-unix/X99 ] && DISPLAY=:99 xdpyinfo >/dev/null 2>&1; then break; fi
        sleep 1
      done
      if ! [ -S /tmp/.X11-unix/X99 ]; then log "XVFB-NOT-READY"; cat /var/log/xvfb.log > "$CONSOLE" 2>&1 || true; exit 1; fi
      DISPLAY=:99 openbox > /var/log/openbox.log 2>&1 &
      for _ in $(seq 1 40); do
        DISPLAY=:99 wmctrl -m >/dev/null 2>&1 && break
        sleep 1
      done
      if DISPLAY=:99 wmctrl -m >/dev/null 2>&1; then
        echo "{ready_marker}" > "$CONSOLE" 2>&1
      else
        log "WMCTRL-NOT-READY"
        cat /var/log/openbox.log > "$CONSOLE" 2>&1 || true
        exit 1
      fi
      log "agent-launch"
      HOME=/root DISPLAY=:99 /usr/local/bin/bridgevm-tools-linux \\
        --device "{dev}" \\
        --token "{token}" \\
        --guest-os linux \\
        {caps} \\
        --no-guest-ip \\
        --no-metrics \\
        > /var/log/bridgevm-agent.log 2>&1 &
      log "agent-pid=$!"
runcmd:
  - [ sh, -c, "/usr/local/bin/run-bridgevm-agent.sh" ]
""")
PY
[[ -s "$SEED_DIR/user-data" ]] || fail "failed to render cloud-init user-data"

rm -f "$SEED_ISO"
hdiutil makehybrid -iso -joliet -default-volume-name cidata -o "$SEED_ISO" "$SEED_DIR" >/dev/null 2>&1 \
  || fail "hdiutil failed to build cidata seed ISO"

rm -f "$BUNDLE/metadata/guest-tools.sock" "$BUNDLE/metadata/qmp.sock" "$BUNDLE/logs/serial.log"
mapfile_args=()
while IFS= read -r line; do mapfile_args+=("$line"); done < <(bridgevm qemu-args "$VM_NAME")
[[ "${#mapfile_args[@]}" -gt 1 ]] || fail "bridgevm qemu-args returned no command"
printf '%s\n' "${mapfile_args[@]}" | grep -Fq "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0" \
  || fail "generated QEMU command is missing the guest-tools virtserialport"
QEMU_PROG="${mapfile_args[0]}"
QEMU_REST=("${mapfile_args[@]:1}")
"$QEMU_PROG" "${QEMU_REST[@]}" \
  -drive "file=$SEED_ISO,if=virtio,format=raw,readonly=on,id=cidata" \
  > "$WORK/qemu.stdout.log" 2>&1 &
QEMU_PID=$!
echo "Launched QEMU pid=$QEMU_PID (store=$STORE)"

set +e
python3 - "$BUNDLE/metadata/guest-tools.sock" "$TIMEOUT_SECONDS" "$APP_ID" "$WINDOW_TITLE" \
  > "$SESSION_LOG" 2> "$SESSION_ERR" <<'PY'
import json, os, socket, sys, time
sock_path, timeout_s, app_id, window_title = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]

deadline = time.time() + timeout_s

def emit(tag, obj):
    print(f"{tag} {obj}", flush=True)

s = None
while time.time() < deadline:
    if os.path.exists(sock_path):
        try:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.connect(sock_path)
            break
        except OSError:
            s = None
    time.sleep(1)
if s is None:
    sys.stderr.write("TIMEOUT: could not connect to guest-tools.sock\n")
    sys.exit(2)

s.settimeout(timeout_s)
buf = b""

def read_line():
    global buf
    while b"\n" not in buf:
        chunk = s.recv(4096)
        if not chunk:
            return None
        buf += chunk
    line, buf = buf.split(b"\n", 1)
    return line.decode("utf-8", "replace")

while time.time() < deadline:
    line = read_line()
    if line is None:
        break
    try:
        env = json.loads(line)
    except json.JSONDecodeError:
        continue
    if isinstance(env.get("message"), dict) and "GuestHello" in env["message"]:
        caps = {c.get("name") for c in env["message"]["GuestHello"].get("capabilities", []) if isinstance(c, dict)}
        if not {"applications", "windows"}.issubset(caps):
            sys.stderr.write(f"GuestHello missing desktop capabilities: {caps}\n")
            sys.exit(3)
        emit("GUEST_HELLO", line)
        break
else:
    sys.stderr.write("TIMEOUT: no GuestHello\n")
    sys.exit(4)

def send(req_id, message):
    cmd = {"protocol_version": 1, "request_id": req_id, "message": message}
    s.sendall((json.dumps(cmd) + "\n").encode("utf-8"))
    emit("SENT", json.dumps(cmd))

def wait_result(req_id):
    while time.time() < deadline:
        line = read_line()
        if line is None:
            break
        try:
            env = json.loads(line)
        except json.JSONDecodeError:
            continue
        msg = env.get("message")
        if isinstance(msg, dict) and "CommandResult" in msg:
            cr = msg["CommandResult"]
            if cr.get("request_id") == req_id:
                emit("RESULT", line)
                return cr
    sys.stderr.write(f"TIMEOUT: missing CommandResult for {req_id}\n")
    sys.exit(5)

send("apps-live-1", "ListApplications")
apps = wait_result("apps-live-1")
assert apps.get("ok") is True, apps
app_entries = apps.get("result", {}).get("applications", [])
assert any(app.get("id") == app_id and app.get("source") == "linux-desktop-file" for app in app_entries), app_entries
assert not any(app.get("id") == "org.bridgevm.hidden.desktop" for app in app_entries), app_entries

send("launch-live-1", {"LaunchApplication": {"id": app_id}})
launch = wait_result("launch-live-1")
assert launch.get("ok") is True, launch
assert launch.get("result", {}).get("application", {}).get("source") == "linux-desktop-file", launch

window_id = None
window_payload = None
for attempt in range(1, 31):
    send(f"windows-live-{attempt}", "ListWindows")
    windows = wait_result(f"windows-live-{attempt}")
    if windows.get("ok") is True:
        for window in windows.get("result", {}).get("windows", []):
            if window.get("title") == window_title and window.get("source") == "wmctrl":
                window_id = window.get("id")
                window_payload = window
                break
    if window_id:
        emit("WINDOW_ID", window_id)
        emit("WINDOW_PAYLOAD", json.dumps(window_payload, sort_keys=True))
        break
    time.sleep(1)
if not window_id:
    sys.stderr.write(f"TIMEOUT: no wmctrl window titled {window_title!r}\n")
    sys.exit(6)

send("focus-live-1", {"FocusWindow": {"id": window_id}})
focus = wait_result("focus-live-1")
assert focus.get("ok") is True, focus
assert focus.get("result", {}).get("window", {}).get("source") == "wmctrl", focus
assert focus.get("result", {}).get("window", {}).get("focused") is True, focus

send("close-live-1", {"CloseWindow": {"id": window_id}})
close = wait_result("close-live-1")
assert close.get("ok") is True, close
assert close.get("result", {}).get("window", {}).get("source") == "wmctrl", close
assert close.get("result", {}).get("window", {}).get("closed") is True, close

emit("ALL_RESULTS_OK", json.dumps({"application": app_id, "window": window_id}))
PY
SESSION_RC=$?
set -e
[[ "$SESSION_RC" -eq 0 ]] || fail "guest-tools session driver failed (rc=$SESSION_RC)"

SERIAL_LOG="$BUNDLE/logs/serial.log"
[[ -f "$SERIAL_LOG" ]] || fail "serial log not found"
serial="$(tr -cd '[:print:]\n' < "$SERIAL_LOG")"
echo "$serial" | grep -q "$DESKTOP_READY_MARKER" || fail "X11 desktop never became ready ($DESKTOP_READY_MARKER)"
echo "$serial" | grep -q "$APP_LAUNCH_MARKER" || fail "gio did not launch the .desktop app ($APP_LAUNCH_MARKER)"
grep -q '^ALL_RESULTS_OK ' "$SESSION_LOG" || fail "session did not report all app/window results ok"
grep -q '^WINDOW_PAYLOAD ' "$SESSION_LOG" || fail "session did not preserve the live window payload"

python3 - "$SESSION_LOG" "$WINDOW_PAYLOAD_JSON" "$CROP_REQUEST_JSON" "$CROP_FRAMEBUFFER_RGBA" <<'PY'
import json
import sys

session_log, payload_path, request_path, framebuffer_path = sys.argv[1:5]
payload = None
with open(session_log, encoding="utf-8") as handle:
    for line in handle:
        if line.startswith("WINDOW_PAYLOAD "):
            payload = json.loads(line.split(" ", 1)[1])
if not isinstance(payload, dict):
    raise SystemExit("missing WINDOW_PAYLOAD")

bounds = payload.get("bounds")
if not isinstance(bounds, dict):
    raise SystemExit("window payload does not include bounds")

x = int(bounds["x"])
y = int(bounds["y"])
width = int(bounds["width"])
height = int(bounds["height"])
if width <= 0 or height <= 0:
    raise SystemExit(f"invalid window bounds: {bounds}")

framebuffer_width = max(1280, x + width, width)
framebuffer_height = max(1024, y + height, height)
host_width = min(width, 960)
host_height = max(1, round(height * host_width / width))
request = {
    "window": payload,
    "framebuffer": {
        "width": framebuffer_width,
        "height": framebuffer_height,
        "pixel_format": "rgba8",
        "source": "synthetic-host-rgba-from-live-wmctrl-bounds",
    },
    "host_size": {"width": host_width, "height": host_height},
}
with open(payload_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, sort_keys=True)
    handle.write("\n")
with open(request_path, "w", encoding="utf-8") as handle:
    json.dump(request, handle, indent=2, sort_keys=True)
    handle.write("\n")

background = bytes([0x18, 0x1A, 0x1F, 0xFF])
window_pixel = bytes([0x23, 0x42, 0x99, 0xFF])
row_bytes = framebuffer_width * 4
framebuffer = bytearray(background * (framebuffer_width * framebuffer_height))
for row in range(max(0, y), min(framebuffer_height, y + height)):
    start = row * row_bytes + max(0, x) * 4
    end = row * row_bytes + min(framebuffer_width, x + width) * 4
    framebuffer[start:end] = window_pixel * ((end - start) // 4)
with open(framebuffer_path, "wb") as handle:
    handle.write(framebuffer)
PY

read_crop_arg() {
  python3 - "$CROP_REQUEST_JSON" "$1" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    request = json.load(handle)
path = sys.argv[2]
value = request
for part in path.split("."):
    value = value[part]
print(value)
PY
}

WINDOW_ID="$(read_crop_arg window.id)"
WINDOW_TITLE_FROM_PAYLOAD="$(read_crop_arg window.title)"
WINDOW_X="$(read_crop_arg window.bounds.x)"
WINDOW_Y="$(read_crop_arg window.bounds.y)"
WINDOW_WIDTH="$(read_crop_arg window.bounds.width)"
WINDOW_HEIGHT="$(read_crop_arg window.bounds.height)"
FRAMEBUFFER_WIDTH="$(read_crop_arg framebuffer.width)"
FRAMEBUFFER_HEIGHT="$(read_crop_arg framebuffer.height)"
HOST_WIDTH="$(read_crop_arg host_size.width)"
HOST_HEIGHT="$(read_crop_arg host_size.height)"

displayd \
  --print-plan \
  --visibility foreground \
  --framebuffer-width "$FRAMEBUFFER_WIDTH" \
  --framebuffer-height "$FRAMEBUFFER_HEIGHT" \
  --scale 1 \
  --window-id "$WINDOW_ID" \
  --window-title "$WINDOW_TITLE_FROM_PAYLOAD" \
  --window-x "$WINDOW_X" \
  --window-y "$WINDOW_Y" \
  --window-width "$WINDOW_WIDTH" \
  --window-height "$WINDOW_HEIGHT" \
  --window-host-width "$HOST_WIDTH" \
  --window-host-height "$HOST_HEIGHT" \
  --framebuffer-rgba-file "$CROP_FRAMEBUFFER_RGBA" \
  --window-crop-rgba-file "$CROP_RGBA" \
  >"$CROP_SUMMARY_JSON" \
  || fail "displayd failed to materialize live window crop artifact"

python3 - "$CROP_SUMMARY_JSON" "$CROP_RGBA" "$CROP_PROOF_JSON" "$WINDOW_PAYLOAD_JSON" "$CROP_REQUEST_JSON" <<'PY'
import hashlib
import json
import os
import sys

summary_path, crop_path, proof_path, payload_path, request_path = sys.argv[1:6]
with open(summary_path, encoding="utf-8") as handle:
    summary = json.load(handle)
with open(payload_path, encoding="utf-8") as handle:
    payload = json.load(handle)
with open(request_path, encoding="utf-8") as handle:
    request = json.load(handle)

frame = summary.get("window_crop_frame")
region = summary.get("window_region")
if not isinstance(frame, dict) or not isinstance(region, dict):
    raise SystemExit("displayd summary does not include window crop artifacts")
expected_bytes = int(frame["output_width"]) * int(frame["output_height"]) * 4
with open(crop_path, "rb") as handle:
    crop = handle.read()
if len(crop) != expected_bytes:
    raise SystemExit(f"crop size mismatch: {len(crop)} != {expected_bytes}")
if crop[:4] != bytes([0x23, 0x42, 0x99, 0xFF]):
    raise SystemExit("crop does not start with the live-window marker color")

proof = {
    "kind": "real-guest-window-proxy-crop-synthetic-framebuffer",
    "proven": True,
    "observation": (
        "A live Linux guest window discovered via wmctrl supplied bounds that "
        "displayd used to materialize a proxy-window RGBA crop artifact from "
        "a synthetic host framebuffer."
    ),
    "limits": [
        "This proves live guest-window metadata drives the crop primitive.",
        "The framebuffer is synthetic; this is not app-direct per-window streaming.",
        "The guest display is Xvfb/openbox under QEMU/HVF, not Apple VZ GUI output.",
    ],
    "window": payload,
    "request": request,
    "artifacts": {
        "window_payload": os.path.basename(payload_path),
        "crop_request": os.path.basename(request_path),
        "framebuffer_rgba": "live-window-framebuffer.rgba",
        "crop_summary": os.path.basename(summary_path),
        "crop_rgba": {
            "artifact": os.path.basename(crop_path),
            "bytes": len(crop),
            "sha256": hashlib.sha256(crop).hexdigest(),
            "width": frame["output_width"],
            "height": frame["output_height"],
            "pixel_format": frame["pixel_format"],
        },
    },
    "displayd": {
        "window_region": region,
        "window_crop_frame": frame,
    },
}
with open(proof_path, "w", encoding="utf-8") as handle:
    json.dump(proof, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

echo "PASS: guest-tools app/window live GUI backend crossed into real Linux desktop tools"
echo "  - ListApplications/LaunchApplication used .desktop + gio ($APP_ID)"
echo "  - ListWindows/FocusWindow/CloseWindow used wmctrl against '$WINDOW_TITLE'"
echo "  - displayd materialized a proxy crop from the live wmctrl bounds:"
echo "    $CROP_PROOF_JSON"
echo "  - all over live virtio-serial transport in a booted Xvfb/openbox guest"
if [[ "$CREATED_STORE" == "1" ]]; then echo "Disposable store preserved: $STORE"; fi
echo "Work dir: $WORK"
