#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-manifest-migration.XXXXXX")"
VM_LOCAL="manifest-migration-local"
VM_SOCKET="manifest-migration-socket"
VM_FUTURE="manifest-migration-future"
VM_MALFORMED="manifest-migration-malformed"
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

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  local status
  set +e
  output="$("$@" 2>&1)"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$label unexpectedly succeeded; got: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

rewrite_schema() {
  local manifest_path="$1"
  local schema="$2"
  python3 - "$manifest_path" "$schema" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
schema = sys.argv[2]
text = path.read_text(encoding="utf-8")
path.write_text(text.replace("bridgevm.io/v1", schema), encoding="utf-8")
PY
}

assert_migration_contract() {
  local label="$1"
  local vm="$2"
  local runner="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local manifest="$bundle/manifest.yaml"
  local backup="$bundle/metadata/manifest-before-migration.yaml"
  local receipt="$bundle/metadata/manifest-migration.json"

  local before_dry_run_checksum
  before_dry_run_checksum="$(cksum <"$manifest")"

  local dry_run
  dry_run="$("$runner" metadata migrate-manifest "$vm" --dry-run)"
  assert_contains "$dry_run" "Manifest migration for $vm" "$label dry-run"
  assert_contains "$dry_run" "Dry run: true" "$label dry-run"
  assert_contains "$dry_run" "Migrated: false" "$label dry-run"
  assert_contains "$dry_run" "From schema: bridgevm.io/v1" "$label dry-run"
  assert_contains "$dry_run" "To schema: bridgevm.io/v1" "$label dry-run"
  [[ "$(cksum <"$manifest")" == "$before_dry_run_checksum" ]] \
    || fail "$label dry-run unexpectedly rewrote manifest"
  [[ ! -f "$backup" ]] || fail "$label dry-run unexpectedly wrote backup"
  [[ ! -f "$receipt" ]] || fail "$label dry-run unexpectedly wrote receipt"

  local migrated
  migrated="$("$runner" metadata migrate-manifest "$vm")"
  assert_contains "$migrated" "Manifest migration for $vm" "$label migrate"
  assert_contains "$migrated" "Dry run: false" "$label migrate"
  assert_contains "$migrated" "Migrated: false" "$label migrate"
  assert_contains "$migrated" "Backup: $backup" "$label migrate"
  assert_contains "$migrated" "Receipt: $receipt" "$label migrate"
  assert_contains "$migrated" "validated: $manifest" "$label migrate"
  assert_contains "$migrated" "backed-up: $backup" "$label migrate"

  [[ -f "$backup" ]] || fail "$label missing manifest backup"
  [[ -f "$receipt" ]] || fail "$label missing manifest migration receipt"
  cmp "$manifest" "$backup" >/dev/null || fail "$label backup differs from current no-op manifest"
  grep -q '"from_schema": "bridgevm.io/v1"' "$receipt" \
    || fail "$label receipt missing from_schema"
  grep -q '"to_schema": "bridgevm.io/v1"' "$receipt" \
    || fail "$label receipt missing to_schema"
  grep -q '"dry_run": false' "$receipt" \
    || fail "$label receipt missing dry_run=false"

  local list_output
  list_output="$("$runner" list)"
  assert_contains "$list_output" "$vm" "$label list after migrate"

  local status_output
  status_output="$("$runner" status "$vm")"
  assert_contains "$status_output" "$vm" "$label status after migrate"
  assert_contains "$status_output" "compatibility" "$label status after migrate"

  local qemu_args
  qemu_args="$("$runner" qemu-args "$vm")"
  assert_contains "$qemu_args" "qemu-system-" "$label qemu-args after migrate"
  assert_contains "$qemu_args" "hostfwd=tcp::2222-:22" "$label qemu-args after migrate"

  local export_path="$STORE-export-${vm}.vmbridge"
  local import_name="${vm}-imported"
  "$runner" export "$vm" --output "$export_path" >/dev/null
  "$runner" import "$export_path" --name "$import_name" >/dev/null
  rm -rf "$export_path"
  local imported_status
  imported_status="$("$runner" status "$import_name")"
  assert_contains "$imported_status" "$import_name" "$label imported status"
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for manifest fixture rewrites"

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm port add "$VM_LOCAL" 2222:22 >/dev/null
mkdir -p "$STORE/workspace"
bridgevm share add "$VM_LOCAL" Workspace "$STORE/workspace" --read-only >/dev/null
assert_migration_contract "local manifest migration" "$VM_LOCAL" bridgevm

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

bridgevm_socket create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm_socket port add "$VM_SOCKET" 2222:22 >/dev/null
mkdir -p "$STORE/socket-workspace"
bridgevm_socket share add "$VM_SOCKET" Workspace "$STORE/socket-workspace" --read-only >/dev/null
assert_migration_contract "socket manifest migration" "$VM_SOCKET" bridgevm_socket

bridgevm create "$VM_FUTURE" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
future_manifest="$STORE/vms/$VM_FUTURE.vmbridge/manifest.yaml"
future_backup="$STORE/vms/$VM_FUTURE.vmbridge/metadata/manifest-before-migration.yaml"
rewrite_schema "$future_manifest" "bridgevm.io/v99"
assert_fails_contains \
  "future schema migration" \
  "manifest schema version must be bridgevm.io/v1" \
  bridgevm metadata migrate-manifest "$VM_FUTURE"
[[ ! -f "$future_backup" ]] \
  || fail "future schema unexpectedly wrote migration backup"
[[ ! -f "$STORE/vms/$VM_FUTURE.vmbridge/metadata/manifest-migration.json" ]] \
  || fail "future schema unexpectedly wrote migration receipt"

bridgevm_socket create "$VM_MALFORMED" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
malformed_manifest="$STORE/vms/$VM_MALFORMED.vmbridge/manifest.yaml"
malformed_backup="$STORE/vms/$VM_MALFORMED.vmbridge/metadata/manifest-before-migration.yaml"
printf 'schemaVersion: bridgevm.io/v1\nname: [not-valid-yaml\n' >"$malformed_manifest"
assert_fails_contains \
  "malformed manifest migration" \
  "YAML error" \
  bridgevm_socket metadata migrate-manifest "$VM_MALFORMED"
[[ ! -f "$malformed_backup" ]] \
  || fail "malformed manifest unexpectedly wrote migration backup"
[[ ! -f "$STORE/vms/$VM_MALFORMED.vmbridge/metadata/manifest-migration.json" ]] \
  || fail "malformed manifest unexpectedly wrote migration receipt"

PRESERVE_STORE=0
echo "PASS: manifest migration CLI/socket integration smoke"
