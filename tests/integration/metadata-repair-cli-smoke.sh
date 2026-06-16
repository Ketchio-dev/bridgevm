#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-metadata-repair.XXXXXX")"
VM_LOCAL="metadata-repair-local"
VM_SOCKET="metadata-repair-socket"
DAEMON_PID=""

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

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

remove_repairable_metadata() {
  local vm="$1"
  local bundle="$STORE/vms/$vm.vmbridge"
  rm -f "$bundle/metadata/state.json"
  rm -f "$bundle/metadata/active-disk.json"
  rm -f "$bundle/metadata/guest-tools-token.json"
  rm -f "$bundle/metadata/snapshot-disks/before-repair.json"
  rm -f "$bundle/metadata/suspend-images/paused-repair.json"
  rm -f "$bundle/metadata/application-consistent-snapshots/app-repair.json"
}

assert_repair_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"

  assert_contains "$output" "Metadata repair for $vm" "$label"
  assert_contains "$output" "Metadata repaired: true" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/state.json" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/active-disk.json" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/guest-tools-token.json" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/snapshot-disks/before-repair.json" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/suspend-images/paused-repair.json" "$label"
  assert_contains "$output" "repaired: $bundle/metadata/application-consistent-snapshots/app-repair.json" "$label"

  [[ -f "$bundle/metadata/state.json" ]] || fail "$label missing state metadata"
  [[ -f "$bundle/metadata/active-disk.json" ]] || fail "$label missing active disk metadata"
  [[ -f "$bundle/metadata/guest-tools-token.json" ]] || fail "$label missing guest-tools token metadata"
  [[ -f "$bundle/metadata/primary-disk.json" ]] || fail "$label missing primary disk metadata"
  [[ -f "$bundle/metadata/snapshot-disks/before-repair.json" ]] \
    || fail "$label missing disk snapshot metadata"
  [[ -f "$bundle/metadata/suspend-images/paused-repair.json" ]] \
    || fail "$label missing suspend image metadata"
  [[ -f "$bundle/metadata/application-consistent-snapshots/app-repair.json" ]] \
    || fail "$label missing application-consistent preflight metadata"

  grep -q '"state": "stopped"' "$bundle/metadata/state.json" \
    || fail "$label state metadata was not reset to stopped"
  grep -q '"source": "primary"' "$bundle/metadata/active-disk.json" \
    || fail "$label active disk metadata was not rebuilt from primary"
  grep -q '"create_command"' "$bundle/metadata/primary-disk.json" \
    || fail "$label primary disk metadata omitted create command"
  grep -q '"fs-freeze"' "$bundle/metadata/application-consistent-snapshots/app-repair.json" \
    || fail "$label application-consistent metadata omitted fs-freeze"

  local token
  token="$(bridgevm guest-tools token "$vm")"
  assert_contains "$token" "Guest tools token for $vm" "$label token"
  assert_contains "$token" "Token: " "$label token"
  token_value="$(printf '%s\n' "$token" | awk -F': ' '/^Token: / {print $2}')"
  [[ "${#token_value}" -eq 64 ]] || fail "$label token was not 64 hex chars: $token"

  local chain
  chain="$(bridgevm snapshot chain "$vm")"
  assert_contains "$chain" "Active disk source: primary" "$label snapshot chain"
  assert_contains "$chain" "Snapshot disk: before-repair" "$label snapshot chain"
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm snapshot create "$VM_LOCAL" before-repair --kind disk >/dev/null
bridgevm snapshot create "$VM_LOCAL" paused-repair --kind suspend >/dev/null
bridgevm snapshot create "$VM_LOCAL" app-repair --kind application-consistent >/dev/null
remove_repairable_metadata "$VM_LOCAL"

local_output="$(bridgevm metadata repair "$VM_LOCAL")"
assert_repair_contract "local metadata repair" "$VM_LOCAL" "$local_output"

local_noop="$(bridgevm metadata repair "$VM_LOCAL")"
assert_contains "$local_noop" "Metadata repaired: false" "local metadata repair noop"
assert_contains "$local_noop" "No metadata repairs needed" "local metadata repair noop"

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
bridgevm snapshot create "$VM_SOCKET" before-repair --kind disk >/dev/null
bridgevm snapshot create "$VM_SOCKET" paused-repair --kind suspend >/dev/null
bridgevm snapshot create "$VM_SOCKET" app-repair --kind application-consistent >/dev/null
remove_repairable_metadata "$VM_SOCKET"

socket_output="$(bridgevm_socket metadata repair "$VM_SOCKET")"
assert_repair_contract "socket metadata repair" "$VM_SOCKET" "$socket_output"

socket_noop="$(bridgevm_socket metadata repair "$VM_SOCKET")"
assert_contains "$socket_noop" "Metadata repaired: false" "socket metadata repair noop"
assert_contains "$socket_noop" "No metadata repairs needed" "socket metadata repair noop"

echo "PASS: metadata repair CLI/socket integration smoke ($STORE)"
