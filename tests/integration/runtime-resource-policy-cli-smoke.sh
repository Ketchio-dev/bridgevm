#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-runtime-resources.XXXXXX")"
VM_NAME="runtime-resources-fast"
APP_DIRECT_VM_NAME="app-direct-display-fast"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""
RUNTIME_CONTROL_SERVER_PID=""
PRESERVE_STORE=1
FAKE_LIGHTVM_RUNNER="$STORE/fake-lightvm-runner"
FAKE_LIGHTVM_ARGS="$STORE/fake-lightvm-runner.args"
FAKE_APPLE_VZ_RUNNER="$STORE/fake-AppleVzRunner"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

lightvm_runner() {
  cargo run --quiet -p lightvm-runner -- --store "$STORE" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
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

write_linux_kernel_raw_manifest() {
  local name="$1"
  local bundle="$2"
  cat >"$bundle/manifest.yaml" <<EOF
schemaVersion: bridgevm.io/v1
name: $name
mode: fast
guest:
  os: ubuntu
  arch: arm64
backend:
  engine: lightvm
  preferred: apple-vz
  fallback: qemu-hvf-restricted
  accelerator: hvf
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.raw
    size: 80GiB
    format: raw
    discard: true
boot:
  mode: linux-kernel
  kernelPath: boot/vmlinuz
  kernelCommandLine: console=hvc0 root=/dev/vda
network:
  mode: nat
  hostname: $name.bridgevm.local
  forwards: []
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
  applications: true
  windows: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
EOF
}

stop_daemon() {
  if [[ -n "${RUNTIME_CONTROL_SERVER_PID:-}" ]]; then
    kill "$RUNTIME_CONTROL_SERVER_PID" 2>/dev/null || true
    wait "$RUNTIME_CONTROL_SERVER_PID" 2>/dev/null || true
  fi
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    if [[ -n "${RUNTIME_CONTROL_SOCKET:-}" ]]; then
      rm -f "$RUNTIME_CONTROL_SOCKET"
    fi
    rm -rf "$STORE"
  fi
}

trap stop_daemon EXIT

command -v python3 >/dev/null || fail "python3 is required for JSON assertions"

bridgevm create "$VM_NAME" \
  --os ubuntu \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-installer \
  --installer-image media/ubuntu.iso >/dev/null

BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
write_linux_kernel_raw_manifest "$VM_NAME" "$BUNDLE"
bridgevm start "$VM_NAME" >/dev/null

POLICY_JSON="$BUNDLE/metadata/runtime-resources.json"
RUNNER_JSON="$BUNDLE/metadata/runner.json"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
RUNTIME_CONTROL_SOCKET="$(
  python3 - "$BUNDLE" <<'PY'
import sys

value = sys.argv[1].encode("utf-8")
hash_value = 0xcbf29ce484222325
for byte in value:
    hash_value ^= byte
    hash_value = (hash_value * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
print(f"/tmp/bvm-vz-{hash_value:016x}.sock")
PY
)"
DISK="$BUNDLE/disks/root.raw"
KERNEL="$BUNDLE/boot/vmlinuz"
mkdir -p "$BUNDLE/metadata"
cat >"$RUNNER_JSON" <<JSON
{
  "engine": "lightvm",
  "pid": 12345,
  "command": ["lightvm-runner"],
  "log_path": "$BUNDLE/logs/lightvm.log",
  "started_at_unix": 1,
  "dry_run": false
}
JSON

background_output="$(
  BRIDGEVM_FORCE_ON_BATTERY=0 bridgevm resources reapply "$VM_NAME" --visibility background
)"
assert_contains "$background_output" "Runtime resources for $VM_NAME" "background policy output"
assert_contains "$background_output" "Visibility: background" "background policy output"
assert_contains "$background_output" "Memory: 2048" "background policy output"
assert_contains "$background_output" "CPU: 1" "background policy output"
assert_contains "$background_output" "Display FPS cap: 10" "background policy output"
assert_contains "$background_output" "Live applied: false" "background policy output"
assert_contains "$background_output" "Runtime control acknowledged: false" "background policy output"
assert_contains "$background_output" "runtime-control-unavailable" "background policy output"

python3 - "$POLICY_JSON" <<'PY' \
  || fail "background runtime resource policy metadata did not match"
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    policy = json.load(handle)

checks = [
    policy.get("visibility") == "background",
    policy.get("state") == "running",
    policy.get("on_battery") is False,
    policy.get("memory") == "2048",
    policy.get("cpu") == "1",
    policy.get("display_fps_cap") == "10",
    policy.get("live_applied") is False,
    (policy.get("live_apply_blockers") or [{}])[0].get("code")
    == "runtime-control-unavailable",
]
sys.exit(0 if all(checks) else 1)
PY

mkdir -p "$(dirname "$DISK")" "$(dirname "$KERNEL")"
: >"$DISK"
: >"$KERNEL"
cat >"$FAKE_LIGHTVM_RUNNER" <<SH
#!/bin/sh
printf '%s\n' "\$@" >"$FAKE_LIGHTVM_ARGS"
sleep 1
SH
chmod +x "$FAKE_LIGHTVM_RUNNER"
cat >"$FAKE_APPLE_VZ_RUNNER" <<'SH'
#!/bin/sh
cat >/dev/null
printf 'fake AppleVzRunner accepted launch\n'
SH
chmod +x "$FAKE_APPLE_VZ_RUNNER"

display_output="$(
  BRIDGEVM_FORCE_ON_BATTERY=0 \
  BRIDGEVM_LIGHTVM_RUNNER="$FAKE_LIGHTVM_RUNNER" \
  BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_APPLE_VZ_RUNNER" \
    bridgevm display "$VM_NAME" --width 1024 --height 768
)"
assert_contains "$display_output" "Launched embedded display window for $VM_NAME" "display launch output"
assert_contains "$display_output" "Dry run: false" "display launch output"
assert_contains "$display_output" "Launch ready: true" "display launch output"
for _ in {1..100}; do
  if [[ -f "$FAKE_LIGHTVM_ARGS" ]]; then
    break
  fi
  sleep 0.02
done
[[ -f "$FAKE_LIGHTVM_ARGS" ]] || fail "display fake runner did not record arguments"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "--apple-vz-display" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "--apple-vz-display-width" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "1024" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "--apple-vz-display-height" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "768" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "--apple-vz-runtime-control-socket" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "$RUNTIME_CONTROL_SOCKET" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "--apple-vz-proxy-framebuffer-rgba-file" "display fake runner args"
assert_contains "$(cat "$FAKE_LIGHTVM_ARGS")" "$BUNDLE/metadata/apple-vz-display-framebuffer.rgba" "display fake runner args"
python3 - "$RUNNER_JSON" "$LAUNCH_SPEC" <<'PY' \
  || fail "display runner metadata did not record launch spec handoff"
import json
import sys

runner_path, launch_spec_path = sys.argv[1:3]
with open(runner_path, encoding="utf-8") as handle:
    runner = json.load(handle)
command = runner.get("command") or []
checks = [
    runner.get("launch_spec_path") == launch_spec_path,
    "--launch-spec" in command,
    launch_spec_path in command,
]
sys.exit(0 if all(checks) else 1)
PY

display_runner_status="$(bridgevm runner-status "$VM_NAME")"
assert_contains "$display_runner_status" "Runtime control kind: apple-vz-display" "display runner-status runtime control"
assert_contains "$display_runner_status" "Runtime control socket: $RUNTIME_CONTROL_SOCKET" "display runner-status runtime control"
assert_contains "$display_runner_status" "Runtime control commands: status, stop, policy, pacing" "display runner-status runtime control"
assert_contains "$display_runner_status" "Runtime policy visibility: foreground" "display runner-status policy"
assert_contains "$display_runner_status" "Runtime policy display FPS cap: adaptive" "display runner-status policy"
assert_contains "$display_runner_status" "Runtime policy live applied: false" "display runner-status policy"
assert_contains "$display_runner_status" "runtime-control-unavailable" "display runner-status policy"

bridgevm create "$APP_DIRECT_VM_NAME" \
  --os ubuntu \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-installer \
  --installer-image media/ubuntu.iso >/dev/null
APP_DIRECT_BUNDLE="$STORE/vms/$APP_DIRECT_VM_NAME.vmbridge"
write_linux_kernel_raw_manifest "$APP_DIRECT_VM_NAME" "$APP_DIRECT_BUNDLE"
bridgevm start "$APP_DIRECT_VM_NAME" >/dev/null
APP_DIRECT_DISK="$APP_DIRECT_BUNDLE/disks/root.raw"
APP_DIRECT_KERNEL="$APP_DIRECT_BUNDLE/boot/vmlinuz"
APP_DIRECT_RUNTIME_CONTROL_SOCKET="$STORE/app-direct-display.sock"
APP_DIRECT_FRAMEBUFFER="$APP_DIRECT_BUNDLE/metadata/apple-vz-display-framebuffer.rgba"
mkdir -p "$(dirname "$APP_DIRECT_DISK")" "$(dirname "$APP_DIRECT_KERNEL")" "$(dirname "$APP_DIRECT_FRAMEBUFFER")"
: >"$APP_DIRECT_DISK"
: >"$APP_DIRECT_KERNEL"
app_direct_output="$(
  lightvm_runner "$APP_DIRECT_VM_NAME" \
    --launch \
    --require-ready \
    --apple-vz-runner "$FAKE_APPLE_VZ_RUNNER" \
    --apple-vz-allow-real-start \
    --apple-vz-display \
    --apple-vz-display-width 640 \
    --apple-vz-display-height 480 \
    --apple-vz-runtime-control-socket "$APP_DIRECT_RUNTIME_CONTROL_SOCKET" \
    --apple-vz-proxy-framebuffer-rgba-file "$APP_DIRECT_FRAMEBUFFER"
)"
assert_contains "$app_direct_output" "fake AppleVzRunner accepted launch" "app-direct lightvm-runner output"
python3 - "$APP_DIRECT_BUNDLE/metadata/runner.json" "$APP_DIRECT_BUNDLE/metadata/state.json" "$APP_DIRECT_RUNTIME_CONTROL_SOCKET" "$APP_DIRECT_FRAMEBUFFER" <<'PY' \
  || fail "app-direct lightvm-runner launch did not record display metadata"
import json
import sys

runner_path, state_path, socket_path, framebuffer_path = sys.argv[1:5]
with open(runner_path, encoding="utf-8") as handle:
    runner = json.load(handle)
with open(state_path, encoding="utf-8") as handle:
    state = json.load(handle)

command = runner.get("command") or []
control = runner.get("runtime_control") or {}
checks = [
    runner.get("engine") == "lightvm",
    runner.get("pid") is not None,
    runner.get("dry_run") is False,
    "--apple-vz-display" in command,
    "--apple-vz-display-width" in command,
    "640" in command,
    "--apple-vz-display-height" in command,
    "480" in command,
    "--apple-vz-proxy-framebuffer-rgba-file" in command,
    framebuffer_path in command,
    control.get("kind") == "apple-vz-display",
    control.get("socket_path") == socket_path,
    control.get("commands") == ["status", "stop", "policy", "pacing"],
    state.get("state") == "stopped",
]
sys.exit(0 if all(checks) else 1)
PY

mkdir -p "$(dirname "$RUNTIME_CONTROL_SOCKET")"
python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            try:
                request = json.loads(data.decode("utf-8"))
            except json.JSONDecodeError as error:
                raise ValueError(f"invalid runtime-control request bytes: {data!r}") from error
            if request.get("command") != "status":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "vm": "runtime-resources-fast",
                    "state": "running",
                    "stopping": False,
                    "display": {"width": 1024, "height": 768},
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready"
runtime_control_status="$(bridgevm runtime-control status "$VM_NAME")"
assert_contains "$runtime_control_status" "Runtime control status for $VM_NAME" "runtime control status output"
assert_contains "$runtime_control_status" "Kind: apple-vz-display" "runtime control status output"
assert_contains "$runtime_control_status" "Socket: $RUNTIME_CONTROL_SOCKET" "runtime control status output"
assert_contains "$runtime_control_status" '"state": "running"' "runtime control status output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "runtime control fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            request = json.loads(data.decode("utf-8"))
            if request.get("command") != "stop":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "vm": "runtime-resources-fast",
                    "accepted": True,
                    "stopping": True,
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control stop request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready for stop"
runtime_control_stop="$(bridgevm runtime-control stop "$VM_NAME")"
assert_contains "$runtime_control_stop" "Runtime control stop for $VM_NAME" "runtime control stop output"
assert_contains "$runtime_control_stop" '"accepted": true' "runtime control stop output"
assert_contains "$runtime_control_stop" '"stopping": true' "runtime control stop output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "runtime control stop fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            request = json.loads(data.decode("utf-8"))
            if request.get("command") != "policy":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "policy": {
                        "vm": "runtime-resources-fast",
                        "visibility": "foreground",
                        "memory": "4096",
                        "cpu": "2",
                        "display_fps_cap": "adaptive",
                        "live_applied": False,
                    },
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control policy request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready for policy"
runtime_control_policy="$(bridgevm runtime-control policy "$VM_NAME")"
assert_contains "$runtime_control_policy" "Runtime control policy for $VM_NAME" "runtime control policy output"
assert_contains "$runtime_control_policy" '"visibility": "foreground"' "runtime control policy output"
assert_contains "$runtime_control_policy" '"display_fps_cap": "adaptive"' "runtime control policy output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "runtime control policy fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            request = json.loads(data.decode("utf-8"))
            if request.get("command") != "pacing":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "visibility": "background",
                    "display_fps_cap": "10",
                    "max_fps": 10,
                    "policy_available": True,
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control pacing request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready for pacing"
runtime_control_pacing="$(bridgevm runtime-control pacing "$VM_NAME")"
assert_contains "$runtime_control_pacing" "Runtime control pacing for $VM_NAME" "runtime control pacing output"
assert_contains "$runtime_control_pacing" '"visibility": "background"' "runtime control pacing output"
assert_contains "$runtime_control_pacing" '"display_fps_cap": "10"' "runtime control pacing output"
assert_contains "$runtime_control_pacing" '"max_fps": 10' "runtime control pacing output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "runtime control pacing fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            request = json.loads(data.decode("utf-8"))
            if request.get("command") != "policy":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "policy": {
                        "vm": "runtime-resources-fast",
                        "visibility": "foreground",
                        "memory": "4096",
                        "cpu": "2",
                        "display_fps_cap": "adaptive",
                        "live_applied": False,
                    },
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control reapply policy request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready for reapply ack"
ack_output="$(
  BRIDGEVM_FORCE_ON_BATTERY=0 bridgevm runtime-control reapply "$VM_NAME" --visibility foreground
)"
assert_contains "$ack_output" "Visibility: foreground" "runtime control ack policy output"
assert_contains "$ack_output" "Runtime control acknowledged: true" "runtime control ack policy output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "runtime control reapply fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

python3 - "$POLICY_JSON" "$RUNNER_JSON" "$RUNTIME_CONTROL_SOCKET" <<'PY' \
  || fail "display launch did not record foreground runtime resource policy"
import json
import sys

policy_path, runner_path, socket_path = sys.argv[1:4]
with open(policy_path, encoding="utf-8") as handle:
    policy = json.load(handle)
with open(runner_path, encoding="utf-8") as handle:
    runner = json.load(handle)

control = runner.get("runtime_control") or {}

checks = [
    policy.get("visibility") == "foreground",
    policy.get("state") == "running",
    policy.get("on_battery") is False,
    policy.get("display_fps_cap") == "adaptive",
    policy.get("live_applied") is False,
    policy.get("runtime_control_acknowledged") is True,
    (policy.get("live_apply_blockers") or [{}])[0].get("code")
    == "runtime-control-unavailable",
    control.get("kind") == "apple-vz-display",
    control.get("socket_path") == socket_path,
    control.get("commands") == ["status", "stop", "policy", "pacing"],
]
sys.exit(0 if all(checks) else 1)
PY

BRIDGEVM_FORCE_ON_BATTERY=1 bridgevmd >"$DAEMON_LOG" 2>&1 &
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

foreground_output="$(bridgevm_socket runtime-control reapply "$VM_NAME" --visibility foreground)"
assert_contains "$foreground_output" "Runtime resources for $VM_NAME" "foreground socket policy output"
assert_contains "$foreground_output" "Visibility: foreground" "foreground socket policy output"
assert_contains "$foreground_output" "On battery: true" "foreground socket policy output"
assert_contains "$foreground_output" "Memory: 2048" "foreground socket policy output"
assert_contains "$foreground_output" "CPU: 1" "foreground socket policy output"
assert_contains "$foreground_output" "Display FPS cap: 10" "foreground socket policy output"
assert_contains "$foreground_output" "Runtime control acknowledged: false" "foreground socket policy output"

python3 - "$POLICY_JSON" <<'PY' \
  || fail "foreground runtime resource policy metadata did not match"
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    policy = json.load(handle)

checks = [
    policy.get("visibility") == "foreground",
    policy.get("on_battery") is True,
    policy.get("memory") == "2048",
    policy.get("cpu") == "1",
    policy.get("display_fps_cap") == "10",
    policy.get("live_applied") is False,
]
sys.exit(0 if all(checks) else 1)
PY

python3 - "$RUNTIME_CONTROL_SOCKET" <<'PY' &
import json
import os
import socket
import sys
import time

path = sys.argv[1]
try:
    os.unlink(path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(path)
server.listen(1)
server.settimeout(10)
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        connection, _ = server.accept()
        with connection:
            chunks = []
            while True:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                chunks.append(chunk)
                if b"\n" in chunk:
                    break
            data = b"".join(chunks)
            if not data:
                continue
            request = json.loads(data.decode("utf-8"))
            if request.get("command") != "status":
                response = {
                    "ok": False,
                    "error": "unexpected-command",
                    "received": request.get("command"),
                }
            else:
                response = {
                    "ok": True,
                    "vm": "runtime-resources-fast",
                    "state": "running",
                    "source": "daemon-socket",
                    "display": {"width": 1024, "height": 768},
                    "supported_commands": ["status", "stop", "policy", "pacing"],
                }
            connection.sendall(json.dumps(response, sort_keys=True).encode("utf-8") + b"\n")
            break
    else:
        raise TimeoutError("no runtime-control request received")
finally:
    server.close()
PY
RUNTIME_CONTROL_SERVER_PID=$!
for _ in {1..100}; do
  if [[ -S "$RUNTIME_CONTROL_SOCKET" ]]; then
    break
  fi
  sleep 0.02
done
[[ -S "$RUNTIME_CONTROL_SOCKET" ]] || fail "runtime control socket was not ready for daemon path"
runtime_control_socket_status="$(bridgevm_socket runtime-control status "$VM_NAME")"
assert_contains "$runtime_control_socket_status" "Runtime control status for $VM_NAME" "daemon runtime control status output"
assert_contains "$runtime_control_socket_status" "Kind: apple-vz-display" "daemon runtime control status output"
assert_contains "$runtime_control_socket_status" "Socket: $RUNTIME_CONTROL_SOCKET" "daemon runtime control status output"
assert_contains "$runtime_control_socket_status" '"source": "daemon-socket"' "daemon runtime control status output"
wait "$RUNTIME_CONTROL_SERVER_PID" || fail "daemon runtime control fake server failed"
RUNTIME_CONTROL_SERVER_PID=""

PRESERVE_STORE=0
echo "PASS: runtime resource policy CLI smoke ($STORE)"
