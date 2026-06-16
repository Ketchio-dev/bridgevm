#!/usr/bin/env bash
set -euo pipefail

# guest-tools-effects-opt-in-smoke.sh
#
# Proves a BridgeVM guest-tools EFFECT actually applies INSIDE a booted Linux
# guest -- not just that the transport carries the envelope. Builds on the
# handshake smoke (guest-tools-live-handshake-opt-in-smoke.sh): same NoCloud
# seed / QEMU boot / guest-tools.sock machinery, then drives the time-sync
# command end to end and ASSERTS the guest clock really moved.
#
# Effect under test: TIME-SYNC.
#   1. The cloud-init launcher deliberately sets the guest clock to a known-
#      WRONG value far in the past (year 2001) right before starting the agent,
#      and starts a tiny in-guest watcher that polls `date +%s` and prints a
#      marker line to /dev/console once the clock jumps forward past a
#      threshold (year 2025+). This is the host-observable ground truth that
#      the guest wall clock changed.
#   2. The host reads the GuestHello off guest-tools.sock, then WRITES a
#      TimeSync command envelope (newline-delimited JSON, carrying the host's
#      current epoch in unix_epoch_millis, with a request_id) back over the
#      same socket. QEMU bridges it to the guest agent.
#   3. The agent applies it with settimeofday(2) (it runs as root under
#      cloud-init) and replies with a CommandResult { ok:true } that echoes the
#      applied epoch in result.applied_unix_epoch_millis.
#
# We assert BOTH:
#   - the agent's CommandResult reply (ok:true, applied epoch ~= host epoch), and
#   - the guest-side watcher marker in the serial log (the clock truly moved),
# so a no-op agent that merely acked could not pass.
#
# OPT-IN (heavy -- boots a real VM). SKIPS unless:
#   BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1
#   BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK=<path to a bootable arm64 Linux cloud qcow2>
#
# Requirements (else skip): macOS hdiutil, qemu-system-aarch64, python3.
#
# Useful overrides (same as the handshake smoke):
#   BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY  (default: target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux)
#   BRIDGEVM_LIVE_GUEST_TOOLS_STORE         (default: disposable mktemp store, preserved for review)
#   BRIDGEVM_LIVE_GUEST_TOOLS_VM            (default: gt-fx; keep it short --
#                                            macOS AF_UNIX paths cap at 104 bytes)
#   BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS (default: 240)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
  exit 0
}

[[ "${BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 to run the guest-tools effects smoke"
[[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK to a bootable arm64 Linux cloud qcow2 (cloud-init enabled)"
[[ -f "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK does not exist: $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK"

command -v qemu-system-aarch64 >/dev/null 2>&1 || \
  skip "qemu-system-aarch64 must be available on PATH"
command -v hdiutil >/dev/null 2>&1 || \
  skip "hdiutil (macOS) is required to build the NoCloud cidata seed ISO"
command -v python3 >/dev/null 2>&1 || \
  skip "python3 is required to drive the guest-tools socket"
qemu-system-aarch64 -accel help 2>/dev/null | grep -q '\bhvf\b' || \
  skip "qemu-system-aarch64 must support the hvf accelerator (Apple Silicon)"

AGENT_BINARY="${BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY:-$ROOT/target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux}"
[[ -f "$AGENT_BINARY" ]] || \
  skip "cross-compiled agent not found at $AGENT_BINARY (build: scripts/build-guest-agent-linux.sh)"

TIMEOUT_SECONDS="${BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS:-240}"
[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS must be a positive integer"

CREATED_STORE=0
if [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_STORE:-}" ]]; then
  STORE="$BRIDGEVM_LIVE_GUEST_TOOLS_STORE"
else
  # Keep this prefix SHORT: macOS caps AF_UNIX socket paths at 104 bytes, and
  # QEMU's guest-tools.sock lives at
  #   $STORE/vms/$VM_NAME.vmbridge/metadata/guest-tools.sock
  # so a long store/VM name makes QEMU refuse to create the chardev socket.
  STORE="$(mktemp -d "/tmp/bvm-fx.XXXXXX")"
  CREATED_STORE=1
fi

# Short default VM name for the same socket-path-length reason (see above).
VM_NAME="${BRIDGEVM_LIVE_GUEST_TOOLS_VM:-gt-fx}"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
WORK="$(mktemp -d "/tmp/bvm-fx-work.XXXXXX")"
SEED_DIR="$WORK/seed"
SEED_ISO="$WORK/cidata-seed.iso"
SESSION_LOG="$WORK/session.log"
SESSION_ERR="$WORK/session.err"
QEMU_PIDFILE="$WORK/qemu.pid"
DEVICE="/dev/virtio-ports/org.bridgevm.guest-tools.0"

# Markers the in-guest watcher prints to /dev/console (captured in serial.log).
WRONG_MARKER="BRIDGEVM-EFFECT-CLOCK-WRONG"   # printed once, with the pre-sync epoch
SYNCED_MARKER="BRIDGEVM-EFFECT-CLOCK-SYNCED" # printed once the clock jumps forward
# Threshold: 2025-01-01T00:00:00Z. The wrong clock (2001) is well below this;
# a successful sync to "now" (2026+) is well above it.
THRESHOLD_EPOCH=1735689600

if [[ -x "$ROOT/target/release/bridgevm" ]]; then
  bridgevm() { "$ROOT/target/release/bridgevm" --store "$STORE" "$@"; }
else
  bridgevm() { cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"; }
fi

QEMU_PID=""

cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$QEMU_PID" 2>/dev/null || break
      sleep 0.5
    done
    kill -9 "$QEMU_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  if [[ -f "$BUNDLE/logs/serial.log" ]]; then
    echo "--- last serial log lines (printable) ---" >&2
    tail -c 6000 "$BUNDLE/logs/serial.log" | tr -cd '[:print:]\n' | tail -40 >&2 || true
  fi
  if [[ -s "$SESSION_ERR" ]]; then
    echo "--- guest-tools session driver stderr ---" >&2
    cat "$SESSION_ERR" >&2 || true
  fi
  if [[ -s "$SESSION_LOG" ]]; then
    echo "--- guest-tools session driver stdout ---" >&2
    cat "$SESSION_LOG" >&2 || true
  fi
  echo "Store preserved at: $STORE" >&2
  echo "Work dir preserved at: $WORK" >&2
  exit 1
}

assert_contains() {
  local haystack="$1" needle="$2" label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

# 1. Create a Compatibility (QEMU) VM and stage the bootable cloud disk.
if [[ ! -d "$BUNDLE" ]]; then
  bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode compatibility >/dev/null \
    || fail "bridgevm create failed"
fi
mkdir -p "$BUNDLE/disks"
cp "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" "$BUNDLE/disks/root.qcow2" \
  || fail "failed to stage root.qcow2 from $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK"

TOKEN_FILE="$BUNDLE/metadata/guest-tools-token.json"
[[ -f "$TOKEN_FILE" ]] || fail "guest-tools token file missing: $TOKEN_FILE"
TOKEN="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["token"])' "$TOKEN_FILE")"
[[ -n "$TOKEN" ]] || fail "empty guest-tools token"

# 2. Build the NoCloud cidata seed ISO embedding the agent + a launcher that:
#    - sets the guest clock to a known-WRONG value (2001) BEFORE the agent,
#    - starts a watcher that prints SYNCED_MARKER once the clock crosses the
#      threshold (proving the wall clock really moved),
#    - launches the agent with default (Real) time-sync.
mkdir -p "$SEED_DIR"
printf 'instance-id: bridgevm-%s\nlocal-hostname: %s\n' "$VM_NAME" "$VM_NAME" > "$SEED_DIR/meta-data"
gzip -c "$AGENT_BINARY" | base64 > "$SEED_DIR/agent.gz.b64" \
  || fail "failed to gzip+base64 the agent"

python3 - "$SEED_DIR" "$TOKEN" "$DEVICE" "$WRONG_MARKER" "$SYNCED_MARKER" "$THRESHOLD_EPOCH" \
  > "$SEED_DIR/user-data" <<'PY'
import sys
seed, token, dev, wrong_marker, synced_marker, threshold = sys.argv[1:7]
with open(f"{seed}/agent.gz.b64") as fh:
    blob = fh.read().strip()
indented = "\n".join("        " + line for line in blob.splitlines())
caps = " ".join(
    f"--capability {c}"
    for c in [
        "heartbeat", "time-sync", "guest-ip", "guest-metrics",
        "fs-freeze", "fs-thaw", "clipboard", "display-resize",
        "shared-folders", "applications", "windows",
    ]
)
print(f"""#cloud-config
write_files:
  - path: /usr/local/bin/bridgevm-tools-linux.gz.b64
    permissions: '0644'
    encoding: text/plain
    content: |
{indented}
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
      ls -l /dev/virtio-ports/ > "$CONSOLE" 2>&1 || true
      if [ ! -e "{dev}" ]; then
        log "DEVICE-MISSING {dev}"
        exit 1
      fi
      # Set the guest clock to a known-WRONG value (2001-01-01) so a real
      # time-sync is observable as a forward jump. date -s @<epoch> is BusyBox-
      # and coreutils-compatible.
      date -s @978307200 > "$CONSOLE" 2>&1 || true
      echo "{wrong_marker} $(date +%s)" > "$CONSOLE" 2>&1
      # Watcher: once the wall clock crosses the threshold, announce it ONCE.
      (
        while :; do
          now=$(date +%s)
          if [ "$now" -ge {threshold} ]; then
            echo "{synced_marker} $now" > "$CONSOLE" 2>&1
            break
          fi
          sleep 1
        done
      ) &
      log "agent-launch"
      /usr/local/bin/bridgevm-tools-linux \\
        --device "{dev}" \\
        --token "{token}" \\
        --guest-os linux \\
        {caps} \\
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
[[ -f "$SEED_ISO" ]] || fail "cidata seed ISO not created: $SEED_ISO"

# 3. Render the bridgevm-generated QEMU command and launch it with the seed ISO
#    attached as an extra virtio block device (same approach as the handshake
#    smoke -- keeps both the guest-tools channel and the cloud-init seed).
rm -f "$BUNDLE/metadata/guest-tools.sock" "$BUNDLE/metadata/qmp.sock" "$BUNDLE/logs/serial.log"
mapfile_args=()
while IFS= read -r line; do
  mapfile_args+=("$line")
done < <(bridgevm qemu-args "$VM_NAME")
[[ "${#mapfile_args[@]}" -gt 1 ]] || fail "bridgevm qemu-args returned no command"

printf '%s\n' "${mapfile_args[@]}" | grep -Fq "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0" \
  || fail "generated QEMU command is missing the guest-tools virtserialport"
printf '%s\n' "${mapfile_args[@]}" | grep -Fq "path=$BUNDLE/metadata/guest-tools.sock" \
  || fail "generated QEMU command is missing the guest-tools.sock chardev path"

QEMU_PROG="${mapfile_args[0]}"
QEMU_REST=("${mapfile_args[@]:1}")

"$QEMU_PROG" "${QEMU_REST[@]}" \
  -drive "file=$SEED_ISO,if=virtio,format=raw,readonly=on,id=cidata" \
  > "$WORK/qemu.stdout.log" 2>&1 &
QEMU_PID=$!
echo "$QEMU_PID" > "$QEMU_PIDFILE"
echo "Launched QEMU pid=$QEMU_PID (store=$STORE)"

# 4. Drive the guest-tools session over guest-tools.sock:
#      - connect (host is the client; QEMU is the server),
#      - read the GuestHello,
#      - WRITE a TimeSync command carrying the host's current epoch millis,
#      - read CommandResult frames until we see ok for our request_id.
#    Prints the captured GuestHello and CommandResult JSON to stdout for the
#    assertions below.
python3 - "$BUNDLE/metadata/guest-tools.sock" "$TIMEOUT_SECONDS" > "$SESSION_LOG" 2> "$SESSION_ERR" <<'PY'
import json
import os
import socket
import sys
import time

sock_path = sys.argv[1]
deadline = time.time() + float(sys.argv[2])
request_id = "smoke-timesync-1"

def emit(tag, payload):
    sys.stdout.write(tag + " " + payload + "\n")
    sys.stdout.flush()

# Connect (retry until QEMU creates the socket and the guest agent connects).
s = None
last_err = None
while time.time() < deadline:
    if not os.path.exists(sock_path):
        time.sleep(1)
        continue
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(sock_path)
        break
    except Exception as exc:
        last_err = exc
        s = None
        time.sleep(1)
if s is None:
    sys.stderr.write(f"could not connect guest-tools.sock; last_err={last_err}\n")
    sys.exit(2)

buf = b""

def next_frame():
    """Read one newline-delimited JSON frame (waiting up to the deadline)."""
    global buf
    while True:
        if b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            line = line.strip()
            if not line:
                continue
            return json.loads(line.decode("utf-8", "replace"))
        if time.time() >= deadline:
            raise TimeoutError("deadline reached waiting for a frame")
        try:
            chunk = s.recv(65536)
        except socket.timeout:
            continue
        if not chunk:
            raise EOFError("guest-tools.sock closed")
        buf += chunk

# Read frames until the GuestHello, then send TimeSync.
sent = False
while time.time() < deadline:
    frame = next_frame()
    msg = frame.get("message", {})
    if not sent and isinstance(msg, dict) and "GuestHello" in msg:
        emit("GUEST_HELLO", json.dumps(frame))
        epoch_millis = int(time.time() * 1000)
        command = {
            "protocol_version": frame.get("protocol_version", 1),
            "request_id": request_id,
            "message": {"TimeSync": {"unix_epoch_millis": epoch_millis}},
        }
        s.sendall((json.dumps(command) + "\n").encode("utf-8"))
        emit("SENT_TIMESYNC", json.dumps(command))
        sent = True
        continue
    if sent and isinstance(msg, dict) and "CommandResult" in msg:
        result = msg["CommandResult"]
        if result.get("request_id") == request_id:
            emit("COMMAND_RESULT", json.dumps(frame))
            s.close()
            sys.exit(0)

sys.stderr.write("TIMEOUT: did not observe CommandResult for our TimeSync\n")
sys.exit(2)
PY
session_status=$?
[[ "$session_status" -eq 0 ]] || fail "guest-tools session driver failed (status $session_status)"
[[ -s "$SESSION_LOG" ]] || fail "empty guest-tools session log"

echo "--- guest-tools session transcript ---"
cat "$SESSION_LOG"

# 5a. Structural assertions on the GuestHello and the agent's CommandResult.
HELLO_LINE="$(grep '^GUEST_HELLO ' "$SESSION_LOG" | head -1 | cut -d' ' -f2-)"
SENT_LINE="$(grep '^SENT_TIMESYNC ' "$SESSION_LOG" | head -1 | cut -d' ' -f2-)"
RESULT_LINE="$(grep '^COMMAND_RESULT ' "$SESSION_LOG" | head -1 | cut -d' ' -f2-)"
[[ -n "$HELLO_LINE" ]] || fail "no GuestHello captured"
[[ -n "$SENT_LINE" ]] || fail "no TimeSync command was sent"
[[ -n "$RESULT_LINE" ]] || fail "no CommandResult captured for the TimeSync"

echo "Exact TimeSync command envelope sent to the guest:"
echo "$SENT_LINE"

python3 - "$HELLO_LINE" "$SENT_LINE" "$RESULT_LINE" "$TOKEN" <<'PY' || fail "agent CommandResult did not confirm time-sync"
import json
import sys

hello = json.loads(sys.argv[1])
sent = json.loads(sys.argv[2])
result_env = json.loads(sys.argv[3])
token = sys.argv[4]

h = hello["message"]["GuestHello"]
assert (h.get("auth") or {}).get("token") == token, "GuestHello token mismatch"

sent_epoch = sent["message"]["TimeSync"]["unix_epoch_millis"]

res = result_env["message"]["CommandResult"]
assert res["request_id"] == "smoke-timesync-1", f"unexpected request_id {res['request_id']!r}"
assert res["ok"] is True, f"agent reported time-sync failure: {res}"
applied = (res.get("result") or {}).get("applied_unix_epoch_millis")
assert applied == sent_epoch, f"applied epoch {applied} != sent {sent_epoch}"
# The agent must have actually run the real backend (not the simulated ack).
msg = (res.get("message") or "")
assert "set guest clock to" in msg, f"unexpected/simulated result message: {msg!r}"
print(f"agent confirmed time-sync ok; applied {applied} ms since epoch")
PY

# 5b. GROUND TRUTH: the in-guest watcher must have observed the wall clock cross
#     the threshold (i.e. the clock REALLY moved from 2001 to now). Without a
#     real settimeofday this marker never appears.
SERIAL_LOG="$BUNDLE/logs/serial.log"
[[ -f "$SERIAL_LOG" ]] || fail "serial log not found at $SERIAL_LOG"

# Confirm the launcher first set the WRONG clock (sanity: the pre-sync state).
grep -aq "$WRONG_MARKER" "$SERIAL_LOG" \
  || fail "guest never reported the deliberately-wrong pre-sync clock ($WRONG_MARKER)"

# Wait (within the deadline) for the SYNCED marker to land in the serial log.
synced=""
sync_deadline=$(( $(date +%s) + 60 ))
while [[ "$(date +%s)" -lt "$sync_deadline" ]]; do
  if synced="$(grep -a "$SYNCED_MARKER" "$SERIAL_LOG" | head -1)"; then
    [[ -n "$synced" ]] && break
  fi
  sleep 1
done
[[ -n "$synced" ]] || fail "guest clock never crossed the threshold ($SYNCED_MARKER missing) -- time-sync did not apply in the guest"

# Extract the observed epoch and assert it is past the threshold.
observed_epoch="$(printf '%s\n' "$synced" | tr -cd '[:print:]\n' | sed -n "s/.*$SYNCED_MARKER \([0-9][0-9]*\).*/\1/p" | head -1)"
[[ -n "$observed_epoch" ]] || fail "could not parse observed guest epoch from: $synced"
[[ "$observed_epoch" -ge "$THRESHOLD_EPOCH" ]] \
  || fail "observed guest epoch $observed_epoch is below threshold $THRESHOLD_EPOCH"

echo "Guest-side ground truth: wall clock moved forward to epoch $observed_epoch (>= $THRESHOLD_EPOCH)"

echo "PASS: guest-tools time-sync effect applied IN-GUEST over real virtio-serial ($STORE)"
echo "  - agent replied CommandResult{ok:true} with applied_unix_epoch_millis"
echo "  - guest wall clock observably jumped from 2001 to current time"
if [[ "$CREATED_STORE" == "1" ]]; then
  echo "Disposable store preserved for review: $STORE"
fi
echo "Work dir (seed ISO, session transcript, QEMU log): $WORK"
