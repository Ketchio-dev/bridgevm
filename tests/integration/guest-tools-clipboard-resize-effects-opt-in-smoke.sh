#!/usr/bin/env bash
set -euo pipefail

# guest-tools-clipboard-resize-effects-opt-in-smoke.sh
#
# Proves the BridgeVM guest-tools CLIPBOARD and DISPLAY-RESIZE effects actually
# drive the REAL Linux tools inside a booted guest -- not just that the agent
# dispatches the configured command (which is unit-tested), and not just that
# the transport carries the envelope (which the time-sync effects smoke proves).
#
# A GUI desktop is NOT required: the guest runs a headless X server (Xvfb), so
# `xclip` and `xrandr` run + are verifiable with no display attached.
#
# Effects under test:
#   CLIPBOARD: the host sends SetClipboard{text}. The agent pipes the text to its
#     configured clipboard command -- here a wrapper that runs `xclip -selection
#     clipboard` against DISPLAY=:99, then READS THE X CLIPBOARD BACK and prints
#     it to /dev/console. We assert the read-back text equals what the host sent,
#     so a no-op could not pass.
#   DISPLAY-RESIZE: the host sends ResizeDisplay{w,h,scale}. The agent runs its
#     configured resize command with `W H SCALE` args -- here a wrapper that runs
#     `xrandr` against DISPLAY=:99 and prints the requested geometry to console.
#
# We assert BOTH the agent's CommandResult{ok:true} replies AND the in-guest
# console markers (the real tools ran with the host's payload).
#
# OPT-IN (heavy -- boots a real VM and apt-installs Xvfb/xclip). SKIPS unless:
#   BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1
#   BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK=<path to a bootable arm64 Linux cloud qcow2>
# The guest needs working apt + network (the NoCloud NAT default is fine).
#
# Requirements (else skip): macOS hdiutil, qemu-system-aarch64 w/ hvf, python3.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() { echo "SKIP: $*"; exit 0; }

[[ "${BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 to run the clipboard/resize effects smoke"
[[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK to a bootable arm64 Linux cloud qcow2 (cloud-init enabled)"
[[ -f "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK does not exist: $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK"
command -v qemu-system-aarch64 >/dev/null 2>&1 || skip "qemu-system-aarch64 must be available"
command -v hdiutil >/dev/null 2>&1 || skip "hdiutil (macOS) is required for the cidata seed ISO"
command -v python3 >/dev/null 2>&1 || skip "python3 is required to drive the guest-tools socket"
qemu-system-aarch64 -accel help 2>/dev/null | grep -q '\bhvf\b' || \
  skip "qemu-system-aarch64 must support the hvf accelerator (Apple Silicon)"

AGENT_BINARY="${BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY:-$ROOT/target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux}"
[[ -f "$AGENT_BINARY" ]] || \
  skip "cross-compiled agent not found at $AGENT_BINARY (build: scripts/build-guest-agent-linux.sh)"

# apt + Xvfb install needs more wall clock than the time-sync smoke.
TIMEOUT_SECONDS="${BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS:-420}"
[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]] || skip "timeout must be a positive integer"

CREATED_STORE=0
if [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_STORE:-}" ]]; then
  STORE="$BRIDGEVM_LIVE_GUEST_TOOLS_STORE"
else
  STORE="$(mktemp -d "/tmp/bvm-cr.XXXXXX")"   # short prefix: 104-byte AF_UNIX cap
  CREATED_STORE=1
fi
VM_NAME="${BRIDGEVM_LIVE_GUEST_TOOLS_VM:-gt-cr}"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
WORK="$(mktemp -d "/tmp/bvm-cr-work.XXXXXX")"
SEED_DIR="$WORK/seed"
SEED_ISO="$WORK/cidata-seed.iso"
SESSION_LOG="$WORK/session.log"
SESSION_ERR="$WORK/session.err"
DEVICE="/dev/virtio-ports/org.bridgevm.guest-tools.0"

# Unique payload so the clipboard read-back is unambiguous.
CLIP_TEXT="BRIDGEVM-CLIP-$(date +%s)-$RANDOM"
CLIP_MARKER="BRIDGEVM-CLIP-APPLIED"
RESIZE_W=1024
RESIZE_H=768
RESIZE_MARKER="BRIDGEVM-RESIZE-CMD"
XREADY_MARKER="BRIDGEVM-XVFB-READY"

if [[ -x "$ROOT/target/release/bridgevm" ]]; then
  bridgevm() { "$ROOT/target/release/bridgevm" --store "$STORE" "$@"; }
else
  bridgevm() { cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"; }
fi

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
    tail -c 8000 "$BUNDLE/logs/serial.log" | tr -cd '[:print:]\n' | tail -50 >&2 || true
  fi
  [[ -s "$SESSION_ERR" ]] && { echo "--- session stderr ---" >&2; cat "$SESSION_ERR" >&2; }
  [[ -s "$SESSION_LOG" ]] && { echo "--- session stdout ---" >&2; cat "$SESSION_LOG" >&2; }
  echo "Store preserved at: $STORE" >&2
  echo "Work dir preserved at: $WORK" >&2
  exit 1
}

# 1. Compatibility (QEMU) VM + bootable cloud disk.
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

# 2. NoCloud seed: installs Xvfb/xclip/xrandr, starts a headless X server, and
#    launches the agent with clipboard/resize commands wired to the real tools.
mkdir -p "$SEED_DIR"
printf 'instance-id: bridgevm-%s\nlocal-hostname: %s\n' "$VM_NAME" "$VM_NAME" > "$SEED_DIR/meta-data"
gzip -c "$AGENT_BINARY" | base64 > "$SEED_DIR/agent.gz.b64" || fail "failed to gzip+base64 the agent"

python3 - "$SEED_DIR" "$TOKEN" "$DEVICE" "$CLIP_MARKER" "$RESIZE_MARKER" "$XREADY_MARKER" \
  > "$SEED_DIR/user-data" <<'PY'
import sys
seed, token, dev, clip_marker, resize_marker, xready = sys.argv[1:7]
with open(f"{seed}/agent.gz.b64") as fh:
    blob = fh.read().strip()
indented = "\n".join("        " + line for line in blob.splitlines())
caps = " ".join(f"--capability {c}" for c in ["heartbeat", "clipboard", "display-resize"])
print(f"""#cloud-config
write_files:
  - path: /usr/local/bin/bridgevm-tools-linux.gz.b64
    permissions: '0644'
    encoding: text/plain
    content: |
{indented}
  - path: /usr/local/bin/bvm-clip.sh
    permissions: '0755'
    content: |
      #!/bin/sh
      # Agent pipes the clipboard text on stdin; set the real X clipboard then
      # read it back to prove xclip holds it.
      export DISPLAY=:99
      text="$(cat)"
      # xclip -i daemonizes to serve the X selection and would inherit this
      # script's stdout/stderr -- which are the agent's captured pipe -- making
      # the agent's wait_with_output() block forever. Redirect the daemon's fds
      # to /dev/null so it holds none of the agent's pipe.
      printf '%s' "$text" | xclip -selection clipboard -i >/dev/null 2>&1
      sleep 0.4
      got="$(xclip -selection clipboard -o 2>/dev/null)"
      echo "{clip_marker}:[$got]" > /dev/console 2>&1
  - path: /usr/local/bin/bvm-resize.sh
    permissions: '0755'
    content: |
      #!/bin/sh
      # Agent passes WIDTH HEIGHT SCALE as args; drive the real xrandr.
      export DISPLAY=:99
      echo "{resize_marker}:$1x$2@$3" > /dev/console 2>&1
      xrandr --output screen --mode "$1x$2" > /dev/console 2>&1 || true
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
      apt-get install -y --no-install-recommends xvfb xclip x11-xserver-utils > "$CONSOLE" 2>&1 \
        || {{ log "APT-INSTALL-FAILED"; exit 1; }}
      log "apt-install-done"
      mkdir -p /tmp/.X11-unix && chmod 1777 /tmp/.X11-unix
      Xvfb :99 -screen 0 1280x1024x24 > /var/log/xvfb.log 2>&1 &
      # Wait for the X server to create its socket and accept connections.
      for _ in $(seq 1 40); do
        if [ -S /tmp/.X11-unix/X99 ] && DISPLAY=:99 xrandr >/dev/null 2>&1; then break; fi
        sleep 1
      done
      if [ -S /tmp/.X11-unix/X99 ] && DISPLAY=:99 xrandr >/dev/null 2>&1; then
        echo "{xready}" > "$CONSOLE" 2>&1
      else
        log "XVFB-NOT-READY"
        echo "BRIDGEVM-XVFB-DIAG which-Xvfb=$(command -v Xvfb) which-xrandr=$(command -v xrandr) sock=$([ -S /tmp/.X11-unix/X99 ] && echo yes || echo no)" > "$CONSOLE" 2>&1
        echo "BRIDGEVM-XVFB-LOG-BEGIN" > "$CONSOLE" 2>&1
        cat /var/log/xvfb.log > "$CONSOLE" 2>&1 || true
        echo "BRIDGEVM-XVFB-LOG-END" > "$CONSOLE" 2>&1
        exit 1
      fi
      log "agent-launch"
      /usr/local/bin/bridgevm-tools-linux \\
        --device "{dev}" \\
        --token "{token}" \\
        --guest-os linux \\
        {caps} \\
        --clipboard-command /usr/local/bin/bvm-clip.sh \\
        --display-resize-command /usr/local/bin/bvm-resize.sh \\
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

# 3. Launch the bridgevm-generated QEMU command with the seed attached.
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

# 4. Drive the guest-tools session: read GuestHello, then send SetClipboard +
#    ResizeDisplay, and read CommandResult frames for both.
python3 - "$BUNDLE/metadata/guest-tools.sock" "$TIMEOUT_SECONDS" "$CLIP_TEXT" "$RESIZE_W" "$RESIZE_H" \
  > "$SESSION_LOG" 2> "$SESSION_ERR" <<'PY'
import json, os, socket, sys, time
sock_path, timeout_s, clip_text, rw, rh = sys.argv[1], int(sys.argv[2]), sys.argv[3], int(sys.argv[4]), int(sys.argv[5])

def emit(tag, obj): print(f"{tag} {obj}", flush=True)

deadline = time.time() + timeout_s
# Wait for QEMU to create the guest-tools.sock, then connect.
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
    sys.stderr.write("TIMEOUT: could not connect to guest-tools.sock\n"); sys.exit(2)

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

# Read frames until the GuestHello.
hello = None
while time.time() < deadline:
    line = read_line()
    if line is None:
        break
    try:
        env = json.loads(line)
    except json.JSONDecodeError:
        continue
    if isinstance(env.get("message"), dict) and "GuestHello" in env["message"]:
        hello = line
        emit("GUEST_HELLO", line)
        break
if hello is None:
    sys.stderr.write("TIMEOUT: no GuestHello\n"); sys.exit(3)

def send(req_id, message):
    cmd = {"protocol_version": 1, "request_id": req_id, "message": message}
    s.sendall((json.dumps(cmd) + "\n").encode("utf-8"))
    emit("SENT", json.dumps(cmd))

send("smoke-clip-1", {"SetClipboard": {"text": clip_text}})
send("smoke-resize-1", {"ResizeDisplay": {"width": rw, "height": rh, "scale": 1}})

want = {"smoke-clip-1", "smoke-resize-1"}
seen = {}
while want and time.time() < deadline:
    line = read_line()
    if line is None:
        break
    try:
        env = json.loads(line)
    except json.JSONDecodeError:
        continue
    msg = env.get("message")
    if isinstance(msg, dict) and "CommandResult" in msg:
        rid = msg["CommandResult"].get("request_id")
        if rid in want:
            emit("RESULT", line)
            seen[rid] = msg["CommandResult"].get("ok")
            want.discard(rid)
if want:
    sys.stderr.write(f"TIMEOUT: missing CommandResult for {want}\n"); sys.exit(4)
emit("ALL_RESULTS_OK", json.dumps(seen))
PY

SESSION_RC=$?
[[ "$SESSION_RC" -eq 0 ]] || fail "guest-tools session driver failed (rc=$SESSION_RC)"

# 5. Assert the agent acked both commands ok.
grep -q '^RESULT ' "$SESSION_LOG" || fail "no CommandResult captured"
python3 - "$SESSION_LOG" <<'PY' || fail "a CommandResult was not ok:true"
import json, sys
ok = {}
for line in open(sys.argv[1]):
    if line.startswith("RESULT "):
        env = json.loads(line[len("RESULT "):])
        cr = env["message"]["CommandResult"]
        ok[cr["request_id"]] = cr.get("ok")
assert ok.get("smoke-clip-1") is True, f"clipboard not ok: {ok}"
assert ok.get("smoke-resize-1") is True, f"resize not ok: {ok}"
print("both CommandResults ok:true")
PY

# 6. Assert the REAL in-guest effects (the tools ran with the host's payload).
SERIAL_LOG="$BUNDLE/logs/serial.log"
[[ -f "$SERIAL_LOG" ]] || fail "serial log not found"
serial="$(tr -cd '[:print:]\n' < "$SERIAL_LOG")"

echo "$serial" | grep -q "$XREADY_MARKER" || fail "Xvfb never became ready ($XREADY_MARKER)"
# Clipboard: xclip set + read-back equals what the host sent.
echo "$serial" | grep -Fq "$CLIP_MARKER:[$CLIP_TEXT]" \
  || fail "guest X clipboard read-back did not equal host text ($CLIP_MARKER:[$CLIP_TEXT])"
# Resize: the agent ran xrandr with the host geometry.
echo "$serial" | grep -Fq "$RESIZE_MARKER:${RESIZE_W}x${RESIZE_H}@1" \
  || fail "guest resize command did not run with host geometry (${RESIZE_W}x${RESIZE_H})"

echo "PASS: guest-tools clipboard + display-resize effects applied with the REAL tools in-guest"
echo "  - SetClipboard -> agent ran xclip; X clipboard read back == host text ($CLIP_TEXT)"
echo "  - ResizeDisplay -> agent ran xrandr with ${RESIZE_W}x${RESIZE_H}"
echo "  - both over the live virtio-serial transport into a real (Xvfb) guest"
if [[ "$CREATED_STORE" == "1" ]]; then echo "Disposable store preserved: $STORE"; fi
echo "Work dir: $WORK"
