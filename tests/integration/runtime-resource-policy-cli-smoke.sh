#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-runtime-resources.XXXXXX")"
VM_NAME="runtime-resources-fast"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""
PRESERVE_STORE=1

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
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

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
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

bridgevm start "$VM_NAME" >/dev/null

BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
POLICY_JSON="$BUNDLE/metadata/runtime-resources.json"
mkdir -p "$BUNDLE/metadata"
cat >"$BUNDLE/metadata/runner.json" <<JSON
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

foreground_output="$(bridgevm_socket resources reapply "$VM_NAME" --visibility foreground)"
assert_contains "$foreground_output" "Runtime resources for $VM_NAME" "foreground socket policy output"
assert_contains "$foreground_output" "Visibility: foreground" "foreground socket policy output"
assert_contains "$foreground_output" "On battery: true" "foreground socket policy output"
assert_contains "$foreground_output" "Memory: 2048" "foreground socket policy output"
assert_contains "$foreground_output" "CPU: 1" "foreground socket policy output"
assert_contains "$foreground_output" "Display FPS cap: 10" "foreground socket policy output"

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

PRESERVE_STORE=0
echo "PASS: runtime resource policy CLI smoke ($STORE)"
