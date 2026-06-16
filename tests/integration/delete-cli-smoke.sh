#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-delete.XXXXXX")"
VM_LOCAL="delete-local"
VM_RUNNING="delete-running"
VM_SOCKET="delete-socket"
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

cleanup() {
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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly contained '$needle'; got: $haystack" ;;
  esac
}

assert_file_exists() {
  local path="$1"
  local label="$2"
  [[ -e "$path" ]] || fail "$label missing: $path"
}

assert_file_absent() {
  local path="$1"
  local label="$2"
  [[ ! -e "$path" ]] || fail "$label unexpectedly exists: $path"
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  assert_file_exists "$path" "$label"
  rg -q --fixed-strings "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2
  local output
  set +e
  output="$("$@" 2>&1)"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

trap cleanup EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm create "$VM_RUNNING" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm start "$VM_RUNNING" >/dev/null

local_bundle="$STORE/vms/$VM_LOCAL.vmbridge"
local_manifest="$local_bundle/manifest.yaml"
local_deletion="$local_bundle/metadata/deletion.json"
local_deleted_manifest="$local_bundle/metadata/deleted-manifest.yaml"

bridgevm delete "$VM_LOCAL" --metadata-only >/dev/null
assert_file_exists "$local_bundle" "local metadata-only delete bundle"
assert_file_exists "$local_manifest" "local metadata-only delete manifest"
assert_file_exists "$local_deletion" "local metadata-only delete metadata"
assert_file_exists "$local_deleted_manifest" "local metadata-only deleted manifest"
assert_file_contains "$local_deletion" "$VM_LOCAL" "local deletion metadata"
assert_file_contains "$local_deleted_manifest" "name: $VM_LOCAL" "local deleted manifest copy"

local_list="$(bridgevm list)"
assert_not_contains "$local_list" "$VM_LOCAL" "local list after metadata-only delete"
assert_fails_contains \
  "local duplicate metadata-only delete" \
  "VM not found: $VM_LOCAL" \
  bridgevm delete "$VM_LOCAL" --metadata-only
assert_file_exists "$local_bundle" "local duplicate delete preserved bundle"
assert_file_exists "$local_manifest" "local duplicate delete preserved manifest"
assert_file_exists "$local_deletion" "local duplicate delete preserved metadata"
assert_file_exists "$local_deleted_manifest" "local duplicate delete preserved deleted manifest"

assert_fails_contains \
  "local running delete" \
  "refusing to delete a running VM; stop it first" \
  bridgevm delete "$VM_RUNNING" --metadata-only
assert_file_exists "$STORE/vms/$VM_RUNNING.vmbridge" "running VM bundle after refused delete"
assert_file_exists "$STORE/vms/$VM_RUNNING.vmbridge/manifest.yaml" "running VM manifest after refused delete"
assert_file_absent "$STORE/vms/$VM_RUNNING.vmbridge/metadata/deletion.json" "running VM deletion metadata"
assert_file_absent "$STORE/vms/$VM_RUNNING.vmbridge/metadata/deleted-manifest.yaml" "running VM deleted manifest copy"

bridgevmd >"$DAEMON_LOG" 2>&1 &
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

bridgevm create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

socket_bundle="$STORE/vms/$VM_SOCKET.vmbridge"
socket_manifest="$socket_bundle/manifest.yaml"
socket_deletion="$socket_bundle/metadata/deletion.json"
socket_deleted_manifest="$socket_bundle/metadata/deleted-manifest.yaml"

bridgevm_socket delete "$VM_SOCKET" --metadata-only >/dev/null
assert_file_exists "$socket_bundle" "socket metadata-only delete bundle"
assert_file_exists "$socket_manifest" "socket metadata-only delete manifest"
assert_file_exists "$socket_deletion" "socket metadata-only delete metadata"
assert_file_exists "$socket_deleted_manifest" "socket metadata-only deleted manifest"
assert_file_contains "$socket_deletion" "$VM_SOCKET" "socket deletion metadata"
assert_file_contains "$socket_deleted_manifest" "name: $VM_SOCKET" "socket deleted manifest copy"

socket_list="$(bridgevm_socket list)"
assert_not_contains "$socket_list" "$VM_SOCKET" "socket list after metadata-only delete"
assert_fails_contains \
  "socket duplicate metadata-only delete" \
  "VM not found: $VM_SOCKET" \
  bridgevm_socket delete "$VM_SOCKET" --metadata-only
assert_file_exists "$socket_bundle" "socket duplicate delete preserved bundle"
assert_file_exists "$socket_manifest" "socket duplicate delete preserved manifest"
assert_file_exists "$socket_deletion" "socket duplicate delete preserved metadata"
assert_file_exists "$socket_deleted_manifest" "socket duplicate delete preserved deleted manifest"

PRESERVE_STORE=0
echo "PASS: delete CLI/socket metadata smoke ($STORE)"
