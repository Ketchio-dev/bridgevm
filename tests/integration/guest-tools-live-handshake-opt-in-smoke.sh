#!/usr/bin/env bash
set -euo pipefail

# guest-tools-live-handshake-opt-in-smoke.sh
#
# Proves the BridgeVM guest-tools transport works END TO END over a real
# virtio-serial channel in a booted Linux guest:
#
#   guest agent (bridgevm-tools-linux)
#     -> /dev/virtio-ports/org.bridgevm.guest-tools.0  (virtserialport)
#     -> QEMU virtio-serial-pci
#     -> host unix socket metadata/guest-tools.sock
#     -> host reads the GuestHello envelope and bridgevm accepts it
#
# The guest is booted with QEMU (HVF, arm64) from a cloud image, and the
# cross-compiled agent is delivered + launched via a NoCloud cloud-init seed
# ISO (volume label "cidata"). The host connects to guest-tools.sock as a
# client, reads the first newline-delimited JSON envelope (the GuestHello),
# and feeds it to `bridgevm guest-tools accept-hello`, which validates the
# tools token against the bundle and the advertised capabilities against the
# VM manifest's agent policy. A tampered token must be rejected.
#
# OPT-IN: this is heavy (boots a real VM). It SKIPS unless:
#   BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1
#   BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK=<path to a bootable arm64 Linux cloud qcow2>
#
# Requirements (else skip): macOS hdiutil, qemu-system-aarch64, python3.
#
# Useful overrides:
#   BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY  (default: target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux)
#   BRIDGEVM_LIVE_GUEST_TOOLS_STORE         (default: disposable mktemp store, preserved for review)
#   BRIDGEVM_LIVE_GUEST_TOOLS_VM            (default: live-guest-tools)
#   BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS (default: 240)
#   BRIDGEVM_LIVE_GUEST_TOOLS_MEMORY_MIB    (informational; the manifest decides RAM)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
  exit 0
}

[[ "${BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 to run the guest-tools live handshake smoke"
[[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK:-}" ]] || \
  skip "set BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK to a bootable arm64 Linux cloud qcow2 (cloud-init enabled)"
[[ -f "$BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK does not exist: $BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK"

command -v qemu-system-aarch64 >/dev/null 2>&1 || \
  skip "qemu-system-aarch64 must be available on PATH"
command -v hdiutil >/dev/null 2>&1 || \
  skip "hdiutil (macOS) is required to build the NoCloud cidata seed ISO"
command -v python3 >/dev/null 2>&1 || \
  skip "python3 is required to read the GuestHello from the guest-tools socket"
qemu-system-aarch64 -accel help 2>/dev/null | grep -q '\bhvf\b' || \
  skip "qemu-system-aarch64 must support the hvf accelerator (Apple Silicon)"

AGENT_BINARY="${BRIDGEVM_LIVE_GUEST_TOOLS_AGENT_BINARY:-$ROOT/target/aarch64-unknown-linux-gnu/release/bridgevm-tools-linux}"
[[ -f "$AGENT_BINARY" ]] || \
  skip "cross-compiled agent not found at $AGENT_BINARY (build: cargo zigbuild -p bridgevm-tools-linux --target aarch64-unknown-linux-gnu --release)"

TIMEOUT_SECONDS="${BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS:-240}"
[[ "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]] || \
  skip "BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS must be a positive integer"

CREATED_STORE=0
if [[ -n "${BRIDGEVM_LIVE_GUEST_TOOLS_STORE:-}" ]]; then
  STORE="$BRIDGEVM_LIVE_GUEST_TOOLS_STORE"
else
  STORE="$(mktemp -d "/tmp/bridgevm-live-guest-tools.XXXXXX")"
  CREATED_STORE=1
fi

VM_NAME="${BRIDGEVM_LIVE_GUEST_TOOLS_VM:-live-guest-tools}"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
WORK="$(mktemp -d "/tmp/bridgevm-live-guest-tools-work.XXXXXX")"
SEED_DIR="$WORK/seed"
SEED_ISO="$WORK/cidata-seed.iso"
HELLO_JSON="$WORK/guest-hello.json"
HELLO_ERR="$WORK/guest-hello.err"
QEMU_PIDFILE="$WORK/qemu.pid"
DEVICE="/dev/virtio-ports/org.bridgevm.guest-tools.0"

# CLI runner. Prefer a prebuilt release binary; fall back to cargo run.
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
    tail -c 4000 "$BUNDLE/logs/serial.log" | tr -cd '[:print:]\n' | tail -25 >&2 || true
  fi
  if [[ -s "$HELLO_ERR" ]]; then
    echo "--- guest-hello reader stderr ---" >&2
    cat "$HELLO_ERR" >&2 || true
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

# 2. Build the NoCloud cidata seed ISO embedding the agent + a launcher.
#    Capabilities are an explicit subset that matches the default manifest's
#    agent policy (no drag-drop / agent-update, which the manifest disables).
mkdir -p "$SEED_DIR"
printf 'instance-id: bridgevm-%s\nlocal-hostname: %s\n' "$VM_NAME" "$VM_NAME" > "$SEED_DIR/meta-data"
gzip -c "$AGENT_BINARY" | base64 > "$SEED_DIR/agent.gz.b64" \
  || fail "failed to gzip+base64 the agent"

python3 - "$SEED_DIR" "$TOKEN" "$DEVICE" > "$SEED_DIR/user-data" <<'PY'
import sys
seed, token, dev = sys.argv[1], sys.argv[2], sys.argv[3]
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

# 3. Render the bridgevm-generated QEMU command and launch it with the seed
#    ISO attached as an extra virtio block device (the generated command does
#    NOT add a cdrom). This keeps the guest-tools virtio-serial channel that
#    bridgevm wires up AND the cloud-init seed both present.
rm -f "$BUNDLE/metadata/guest-tools.sock" "$BUNDLE/metadata/qmp.sock" "$BUNDLE/logs/serial.log"
mapfile_args=()
while IFS= read -r line; do
  mapfile_args+=("$line")
done < <(bridgevm qemu-args "$VM_NAME")
[[ "${#mapfile_args[@]}" -gt 1 ]] || fail "bridgevm qemu-args returned no command"

# Sanity: the generated command must include the guest-tools channel.
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

# 4. Read the GuestHello off guest-tools.sock (host connects as a CLIENT;
#    QEMU is the server). The agent writes GuestHello on connect; QEMU buffers
#    until the host connects, so a late connect still receives it.
python3 - "$BUNDLE/metadata/guest-tools.sock" "$TIMEOUT_SECONDS" > "$HELLO_JSON" 2> "$HELLO_ERR" <<'PY'
import os
import socket
import sys
import time

sock_path = sys.argv[1]
deadline = time.time() + float(sys.argv[2])
buf = b""
last_err = None
while time.time() < deadline:
    if not os.path.exists(sock_path):
        time.sleep(1)
        continue
    try:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(5)
        s.connect(sock_path)
    except Exception as exc:
        last_err = exc
        time.sleep(1)
        continue
    try:
        while time.time() < deadline:
            try:
                chunk = s.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                break
            buf += chunk
            if b"\n" in buf:
                sys.stdout.write(buf.split(b"\n", 1)[0].decode("utf-8", "replace"))
                sys.stdout.flush()
                s.close()
                sys.exit(0)
    except Exception as exc:
        last_err = exc
    finally:
        try:
            s.close()
        except Exception:
            pass
    time.sleep(1)

sys.stderr.write(f"TIMEOUT waiting for GuestHello; last_err={last_err}; bytes={len(buf)}\n")
if buf:
    sys.stderr.write("partial: " + buf.decode("utf-8", "replace") + "\n")
sys.exit(2)
PY
hello_status=$?
[[ "$hello_status" -eq 0 ]] || fail "did not receive GuestHello over guest-tools.sock within ${TIMEOUT_SECONDS}s"
[[ -s "$HELLO_JSON" ]] || fail "empty GuestHello captured from guest-tools.sock"

HELLO="$(cat "$HELLO_JSON")"
echo "Received GuestHello over guest-tools.sock:"
echo "$HELLO"

# Structural sanity on the captured envelope.
python3 - "$HELLO_JSON" "$TOKEN" <<'PY' || exit 1
import json
import sys
data = json.load(open(sys.argv[1]))
msg = data.get("message", {})
hello = msg.get("GuestHello")
assert hello is not None, f"expected GuestHello, got keys {list(msg.keys())}"
auth = hello.get("auth") or {}
assert auth.get("token") == sys.argv[2], "GuestHello token does not match bundle token"
assert hello.get("guest_os") == "linux", f"unexpected guest_os {hello.get('guest_os')!r}"
assert len(hello.get("capabilities", [])) >= 1, "GuestHello advertised no capabilities"
PY

# 5. Host accepts the GuestHello: token validated against the bundle, and the
#    advertised capabilities validated against the VM manifest's agent policy.
accept_output="$(bridgevm guest-tools accept-hello "$VM_NAME" --hello-json "$HELLO")" \
  || fail "bridgevm guest-tools accept-hello rejected a valid live GuestHello"
echo "$accept_output"
assert_contains "$accept_output" "Accepted guest tools session for $VM_NAME" "accept-hello"
assert_contains "$accept_output" "Guest OS: linux" "accept-hello"
assert_contains "$accept_output" "Capability: heartbeat" "accept-hello"

# 6. Negative control: a tampered token MUST be rejected (proves real validation).
TAMPERED="$(python3 - "$HELLO_JSON" <<'PY'
import json
import sys
data = json.load(open(sys.argv[1]))
data["message"]["GuestHello"]["auth"]["token"] = "0" * 64
print(json.dumps(data))
PY
)"
if bridgevm guest-tools accept-hello "$VM_NAME" --hello-json "$TAMPERED" >/dev/null 2>&1; then
  fail "tampered-token GuestHello was accepted (token validation is not enforced)"
fi
echo "Negative control: tampered-token GuestHello correctly rejected"

echo "PASS: guest-tools live handshake over real virtio-serial channel ($STORE)"
if [[ "$CREATED_STORE" == "1" ]]; then
  echo "Disposable store preserved for review: $STORE"
fi
echo "Work dir (seed ISO, captured GuestHello, QEMU log): $WORK"
