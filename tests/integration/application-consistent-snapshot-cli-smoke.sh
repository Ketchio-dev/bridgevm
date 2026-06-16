#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-app-consistent-snapshot.XXXXXX")"
VM_LOCAL="app-consistent-local"
VM_PARTIAL="app-consistent-partial"
VM_SOCKET="app-consistent-socket"

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
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
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

assert_preflight_contract() {
  local label="$1"
  local vm="$2"
  local snapshot="$3"
  local output="$4"
  local metadata="$STORE/vms/$vm.vmbridge/metadata/application-consistent-snapshots/$snapshot.json"

  assert_contains "$output" "Created application-consistent snapshot '$snapshot'" "$label"
  assert_contains "$output" "Application-consistent preflight: $snapshot" "$label"
  assert_contains "$output" "Guest tools connected: false" "$label"
  assert_contains "$output" "Required capabilities: fs-freeze, fs-thaw" "$label"
  assert_contains "$output" "Missing capabilities: fs-freeze, fs-thaw" "$label"
  assert_contains "$output" "Application-consistent ready: false" "$label"
  assert_contains "$output" "future guest-agent request to freeze guest filesystems" "$label"
  assert_contains "$output" "future guest-agent request to thaw guest filesystems" "$label"

  [[ -f "$metadata" ]] || fail "$label metadata missing: $metadata"
  grep -q '"ready": false' "$metadata" || fail "$label metadata omitted ready=false"
  grep -q '"connected": false' "$metadata" || fail "$label metadata omitted connected=false"
  grep -q '"fs-freeze"' "$metadata" || fail "$label metadata omitted fs-freeze"
  grep -q '"fs-thaw"' "$metadata" || fail "$label metadata omitted fs-thaw"
  grep -q '"planned_freeze_semantics"' "$metadata" \
    || fail "$label metadata omitted planned_freeze_semantics"
  grep -q '"planned_thaw_semantics"' "$metadata" \
    || fail "$label metadata omitted planned_thaw_semantics"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
local_output="$(bridgevm snapshot create "$VM_LOCAL" before-app --kind application-consistent)"
assert_preflight_contract "local application-consistent snapshot" "$VM_LOCAL" before-app "$local_output"

set +e
duplicate_output="$(
  bridgevm snapshot create "$VM_LOCAL" before-app --kind application-consistent 2>&1
)"
duplicate_status=$?
set -e
[[ "$duplicate_status" -ne 0 ]] \
  || fail "duplicate application-consistent snapshot unexpectedly succeeded"
assert_contains \
  "$duplicate_output" \
  "snapshot already exists for $VM_LOCAL: before-app" \
  "duplicate application-consistent snapshot"

set +e
local_execute_output="$(
  bridgevm snapshot execute-application-consistent "$VM_LOCAL" before-app-execute 2>&1
)"
local_execute_status=$?
set -e
[[ "$local_execute_status" -ne 0 ]] \
  || fail "local execute-application-consistent unexpectedly succeeded"
assert_contains \
  "$local_execute_output" \
  "application-consistent snapshot execution requires --socket bridgevmd access" \
  "local execute-application-consistent guard"

bridgevm create "$VM_PARTIAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
cat >"$STORE/vms/$VM_PARTIAL.vmbridge/metadata/guest-tools-runtime.json" <<JSON
{
  "connected": true,
  "guest_os": "linux",
  "agent_version": "smoke-partial",
  "capabilities": ["heartbeat", "fs-freeze"],
  "last_heartbeat_at_unix": 12344,
  "guest_ip_addresses": [],
  "shared_folders": [],
  "metrics": null,
  "updated_at_unix": 12345
}
JSON
partial_output="$(
  bridgevm snapshot create "$VM_PARTIAL" partial-app --kind application-consistent
)"
assert_contains "$partial_output" "Guest tools connected: true" \
  "partial capability application-consistent snapshot"
assert_contains "$partial_output" "Available capabilities: heartbeat, fs-freeze" \
  "partial capability application-consistent snapshot"
assert_contains "$partial_output" "Missing capabilities: fs-thaw" \
  "partial capability application-consistent snapshot"
assert_contains "$partial_output" "Application-consistent ready: false" \
  "partial capability application-consistent snapshot"
assert_contains "$partial_output" "Guest tools runtime updated: 12345" \
  "partial capability application-consistent snapshot"
partial_metadata="$STORE/vms/$VM_PARTIAL.vmbridge/metadata/application-consistent-snapshots/partial-app.json"
[[ -f "$partial_metadata" ]] || fail "partial capability metadata missing: $partial_metadata"
grep -q '"connected": true' "$partial_metadata" \
  || fail "partial capability metadata omitted connected=true"
grep -q '"ready": false' "$partial_metadata" \
  || fail "partial capability metadata omitted ready=false"
grep -q '"runtime_updated_at_unix": 12345' "$partial_metadata" \
  || fail "partial capability metadata omitted runtime timestamp"
grep -q '"fs-freeze"' "$partial_metadata" \
  || fail "partial capability metadata omitted available fs-freeze"
grep -q '"fs-thaw"' "$partial_metadata" \
  || fail "partial capability metadata omitted missing fs-thaw"

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

bridgevm create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
socket_output="$(
  bridgevm_socket snapshot create "$VM_SOCKET" before-app --kind application-consistent
)"
assert_preflight_contract \
  "socket application-consistent snapshot" \
  "$VM_SOCKET" \
  before-app \
  "$socket_output"

echo "PASS: application-consistent snapshot CLI/socket smoke ($STORE)"
