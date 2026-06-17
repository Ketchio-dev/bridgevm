#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-qmp-supervisor.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="qmp-supervisor"
VM_RESUME_FAIL="qmp-resume-fail"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-system-x86_64" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

python3 - "$@" <<'PY'
import os
import signal
import socket
import sys
import time

args = sys.argv[1:]
log_path = os.environ["BRIDGEVM_FAKE_BACKEND_LOG"]
with open(log_path, "a", encoding="utf-8") as log:
    log.write("qemu-system-x86_64 " + " ".join(args) + "\n")

if os.path.exists(log_path + ".fail-loadvm") and "-loadvm" in args:
    raise SystemExit(42)

qmp_arg = None
for index, arg in enumerate(args):
    if arg == "-qmp" and index + 1 < len(args):
        qmp_arg = args[index + 1]
        break

if not qmp_arg or not qmp_arg.startswith("unix:"):
    raise SystemExit(f"missing QMP unix socket argument: {args!r}")

socket_path = qmp_arg.removeprefix("unix:").split(",", 1)[0]
os.makedirs(os.path.dirname(socket_path), exist_ok=True)
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

stop = False

def mark_stop(_signum, _frame):
    global stop
    stop = True

signal.signal(signal.SIGTERM, mark_stop)

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(1)
server.settimeout(15)

conn, _ = server.accept()
with conn:
    stream = conn.makefile("rwb")
    stream.write(b'{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}}}}\n')
    stream.flush()
    capabilities = stream.readline().decode("utf-8")
    if "qmp_capabilities" not in capabilities:
        raise SystemExit(f"missing qmp_capabilities: {capabilities!r}")
    stream.write(b'{"return":{}}\n')
    stream.write(b'{"event":"RESUME","data":{"status":"running"}}\n')
    stream.write(b'{"event":"SHUTDOWN","data":{"guest":true}}\n')
    stream.flush()

deadline = time.monotonic() + 30
while not stop and time.monotonic() < deadline:
    time.sleep(0.05)
PY
SH
chmod +x "$FAKE_BIN/qemu-system-x86_64"

for backend in qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "unexpected backend in qmp supervisor smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE" --reconcile-interval-ms 25
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
    cat "$DAEMON_LOG" >&2
  fi
  if [[ -f "$BACKEND_LOG" ]]; then
    echo "Backend log: $BACKEND_LOG" >&2
    cat "$BACKEND_LOG" >&2
  fi
  exit 1
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

assert_contains_any() {
  local haystack="$1"
  local label="$2"
  shift 2
  local needle
  for needle in "$@"; do
    case "$haystack" in
      *"$needle"*) return ;;
    esac
  done
  fail "$label missing one of '$*'; got: $haystack"
}

assert_qmp_supervisor_diagnostic() {
  local output="$1"
  local label="$2"
  assert_contains "$output" "QMP supervisor" "$label"
  assert_contains_any "$output" "$label" "events" "Events"
  assert_contains "$output" "RESUME" "$label"
  assert_contains "$output" "SHUTDOWN" "$label"
  assert_contains_any "$output" "$label" "terminal" "Terminal"
  assert_contains_any "$output" "$label" "envelopes_read: 2" "Envelopes read: 2" "envelopes read: 2"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..600}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

bridgevm_socket create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
DISK="$BUNDLE/disks/root.qcow2"
SUPERVISOR_METADATA="$BUNDLE/metadata/qmp-supervisor.json"
mkdir -p "$(dirname "$DISK")"
printf 'fake qcow2 disk for qmp supervisor smoke\n' >"$DISK"

run_output="$(bridgevm_socket run "$VM_NAME" --spawn)"
assert_contains "$run_output" "Engine: fullvm" "socket run spawn"
assert_contains "$run_output" "Dry run: false" "socket run spawn"
assert_contains "$run_output" "PID:" "socket run spawn"
assert_contains "$run_output" "Guest tools transport: virtio-serial" "socket run spawn"
assert_contains "$run_output" "Guest tools channel: org.bridgevm.guest-tools.0" "socket run spawn"
assert_contains "$run_output" "Guest tools socket: $BUNDLE/metadata/guest-tools.sock" "socket run spawn"
assert_contains "$run_output" "Command: qemu-system-x86_64" "socket run spawn"
assert_contains "$run_output" "-display vnc=:0" "socket run spawn"
assert_contains "$run_output" "org.bridgevm.guest-tools.0" "socket run spawn"

for _ in {1..200}; do
  if [[ -f "$SUPERVISOR_METADATA" ]]; then
    break
  fi
  bridgevm_socket runner-status "$VM_NAME" >/dev/null || true
  sleep 0.05
done

[[ -f "$SUPERVISOR_METADATA" ]] || fail "QMP supervisor metadata was not written"

python3 - "$SUPERVISOR_METADATA" <<'PY'
import json
import sys

metadata_path = sys.argv[1]
with open(metadata_path, encoding="utf-8") as handle:
    metadata = json.load(handle)

events = [event.get("name") for event in metadata.get("events", [])]
if events != ["RESUME", "SHUTDOWN"]:
    raise SystemExit(f"unexpected QMP event sequence: {events!r}")
if metadata.get("terminal_event", {}).get("name") != "SHUTDOWN":
    raise SystemExit(f"missing SHUTDOWN terminal event: {metadata!r}")
if metadata.get("terminal_event", {}).get("data") != {"guest": True}:
    raise SystemExit(f"unexpected terminal event data: {metadata!r}")
if metadata.get("envelopes_read") != 2:
    raise SystemExit(f"unexpected envelopes_read: {metadata!r}")
if metadata.get("limit_reached"):
    raise SystemExit(f"drain limit should not be reached: {metadata!r}")
if not isinstance(metadata.get("updated_at_unix"), int):
    raise SystemExit(f"missing updated_at_unix: {metadata!r}")
PY

status_diagnostic="$(bridgevm_socket status "$VM_NAME")"
assert_qmp_supervisor_diagnostic "$status_diagnostic" "status QMP supervisor diagnostic"

for _ in {1..200}; do
  runner_status="$(bridgevm_socket runner-status "$VM_NAME")"
  if [[ "$runner_status" == *"No runner metadata"* ]]; then
    break
  fi
  sleep 0.05
done

assert_contains "$runner_status" "No runner metadata" "runner cleanup after terminal QMP event"
assert_qmp_supervisor_diagnostic "$runner_status" "runner-status QMP supervisor diagnostic"

status_output="$(bridgevm_socket status "$VM_NAME")"
assert_contains "$status_output" "$VM_NAME" "status after terminal QMP event"
assert_contains "$status_output" "stopped" "status after terminal QMP event"
assert_qmp_supervisor_diagnostic "$status_output" "status after terminal QMP event"

readiness_output="$(bridgevm_socket readiness "$VM_NAME")"
assert_contains "$readiness_output" "Readiness report for $VM_NAME" "readiness QMP supervisor diagnostic"
assert_contains "$readiness_output" "Metadata only: true" "readiness QMP supervisor diagnostic"
assert_qmp_supervisor_diagnostic "$readiness_output" "readiness QMP supervisor diagnostic"

[[ -s "$BACKEND_LOG" ]] || fail "fake QEMU backend was not launched"
assert_contains "$(cat "$BACKEND_LOG")" "-qmp unix:$BUNDLE/metadata/qmp.sock,server=on,wait=off" "fake backend QMP args"
assert_contains "$(cat "$BACKEND_LOG")" "-display vnc=:0" "fake backend VNC display args"
assert_contains "$(cat "$BACKEND_LOG")" "socket,id=bridgevm-tools,path=$BUNDLE/metadata/guest-tools.sock,server=on,wait=off" "fake backend guest tools chardev args"
assert_contains "$(cat "$BACKEND_LOG")" "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0" "fake backend guest tools device args"

bridgevm_socket create "$VM_RESUME_FAIL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
RESUME_BUNDLE="$STORE/vms/$VM_RESUME_FAIL.vmbridge"
RESUME_DISK="$RESUME_BUNDLE/disks/root.qcow2"
RESUME_MARKER="$RESUME_BUNDLE/metadata/suspend-images/$VM_RESUME_FAIL-compat.json"
mkdir -p "$(dirname "$RESUME_DISK")" "$(dirname "$RESUME_MARKER")"
printf 'fake qcow2 disk for failed resume\n' >"$RESUME_DISK"
printf '{"snapshot_tag":"bridgevm-suspend","disk":"%s"}\n' "$RESUME_DISK" >"$RESUME_MARKER"
printf '{"state":"suspended","updated_at_unix":1}\n' >"$RESUME_BUNDLE/metadata/state.json"

touch "$BACKEND_LOG.fail-loadvm"
if resume_failure="$(bridgevm_socket resume "$VM_RESUME_FAIL" 2>&1)"; then
  fail "socket resume unexpectedly succeeded after fake -loadvm failure: $resume_failure"
fi
assert_contains "$resume_failure" "Compatibility Mode resume failed: QEMU exited" "failed compat resume"
assert_contains "$resume_failure" "suspend snapshot is preserved" "failed compat resume"
[[ -f "$RESUME_MARKER" ]] || fail "failed compat resume consumed suspend marker"
resume_status="$(bridgevm_socket status "$VM_RESUME_FAIL")"
assert_contains "$resume_status" "$VM_RESUME_FAIL" "failed compat resume state"
assert_contains "$resume_status" "suspended" "failed compat resume state"
resume_runner_status="$(bridgevm_socket runner-status "$VM_RESUME_FAIL")"
assert_contains "$resume_runner_status" "No runner metadata" "failed compat resume runner metadata"
assert_contains "$(cat "$BACKEND_LOG")" "-loadvm bridgevm-suspend" "failed compat resume loadvm args"

echo "PASS: qmp supervisor CLI/socket metadata smoke ($STORE)"
