#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-snapshot-list-restore.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="snapshot-list-restore-local"
VM_SOCKET="snapshot-list-restore-socket"
SNAPSHOT_NAME="before-change"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    backing=""
    for ((i = 1; i <= $#; i++)); do
      if [[ "${!i}" == "-b" ]]; then
        next=$((i + 1))
        backing="${!next}"
      fi
    done
    if [[ -n "$backing" ]]; then
      path="${@: -1}"
    else
      path="${@: -2:1}"
    fi
    mkdir -p "$(dirname "$path")"
    if [[ -n "$backing" ]]; then
      [[ -f "$backing" ]] || {
        echo "missing backing $backing" >&2
        exit 3
      }
      printf 'fake snapshot overlay backed by %s\n' "$backing" >"$path"
    else
      printf 'fake primary disk\n' >"$path"
    fi
    echo "created $path"
    ;;
  *)
    echo "unsupported qemu-img invocation: $*" >&2
    exit 64
    ;;
esac
SH
chmod +x "$FAKE_BIN/qemu-img"

export PATH="$FAKE_BIN:$PATH"

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

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label missing file: $path"
  grep -q "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

assert_snapshot_list_restore_contract() {
  local label="$1"
  local vm="$2"
  local runner="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local primary_disk="$bundle/disks/root.qcow2"
  local snapshot_metadata="$bundle/metadata/snapshot-disks/$SNAPSHOT_NAME.json"
  local active_disk_metadata="$bundle/metadata/active-disk.json"
  local last_restore="$bundle/metadata/last-restore.json"

  "$runner" create "$vm" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
  "$runner" disk create "$vm" >/dev/null
  [[ -f "$primary_disk" ]] || fail "$label primary disk was not created"

  "$runner" start "$vm" >/dev/null
  create_output="$("$runner" snapshot create "$vm" "$SNAPSHOT_NAME" --kind disk)"
  assert_contains "$create_output" "Created disk snapshot '$SNAPSHOT_NAME'" "$label create"
  assert_contains "$create_output" "Snapshot disk overlay:" "$label create"
  assert_contains "$create_output" "Snapshot disk backing: $primary_disk" "$label create"

  chain_before_create="$("$runner" snapshot chain "$vm")"
  assert_contains "$chain_before_create" "Active disk source: primary" "$label chain before create"
  assert_contains "$chain_before_create" "Active disk: $primary_disk" "$label chain before create"
  assert_contains "$chain_before_create" "Snapshot disk: $SNAPSHOT_NAME" "$label chain before create"
  assert_contains "$chain_before_create" "Snapshot disk overlay ready: false" "$label chain before create"
  assert_contains "$chain_before_create" "Snapshot disk backing ready: true" "$label chain before create"

  list_output="$("$runner" snapshot list "$vm")"
  assert_contains "$list_output" "$SNAPSHOT_NAME" "$label list"
  assert_contains "$list_output" "disk" "$label list"
  assert_contains "$list_output" "running" "$label list"

  assert_fails_contains \
    "$label missing snapshot restore" \
    "snapshot not found for $vm: missing-snapshot" \
    "$runner" snapshot restore "$vm" missing-snapshot

  "$runner" snapshot disk-create "$vm" "$SNAPSHOT_NAME" >/dev/null
  assert_file_contains "$snapshot_metadata" '"overlay_exists": true' "$label snapshot metadata"
  assert_file_contains "$snapshot_metadata" '"backing_exists": true' "$label snapshot metadata"
  assert_file_contains "$active_disk_metadata" '"source": "snapshot-overlay"' "$label active disk metadata"
  assert_file_contains "$active_disk_metadata" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label active disk metadata"

  chain_after_create="$("$runner" snapshot chain "$vm")"
  assert_contains "$chain_after_create" "Active disk source: snapshot-overlay" "$label chain after create"
  assert_contains "$chain_after_create" "Active disk snapshot: $SNAPSHOT_NAME" "$label chain after create"
  assert_contains "$chain_after_create" "Snapshot disk overlay ready: true" "$label chain after create"

  "$runner" stop "$vm" >/dev/null
  restore_output="$("$runner" snapshot restore "$vm" "$SNAPSHOT_NAME")"
  assert_contains "$restore_output" "Restored snapshot '$SNAPSHOT_NAME' metadata" "$label restore"
  assert_contains "$restore_output" "recorded state: running" "$label restore"
  assert_contains "$restore_output" "Active disk source: snapshot-backing" "$label restore"
  assert_contains "$restore_output" "Active disk snapshot: $SNAPSHOT_NAME" "$label restore"
  assert_contains "$restore_output" "Active disk: $primary_disk" "$label restore"
  assert_contains "$restore_output" "Active disk ready: true" "$label restore"

  assert_file_contains "$last_restore" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label last restore"
  assert_file_contains "$last_restore" '"restored_state": "running"' "$label last restore"
  assert_file_contains "$last_restore" '"source": "snapshot-backing"' "$label last restore"
  assert_file_contains "$active_disk_metadata" '"source": "snapshot-backing"' "$label restored active disk metadata"
  assert_file_contains "$active_disk_metadata" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label restored active disk metadata"

  status_output="$("$runner" status "$vm")"
  assert_contains "$status_output" "running" "$label restored status"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

assert_snapshot_list_restore_contract \
  "local snapshot list/restore" \
  "$VM_LOCAL" \
  bridgevm

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

assert_snapshot_list_restore_contract \
  "socket snapshot list/restore" \
  "$VM_SOCKET" \
  bridgevm_socket

echo "PASS: snapshot list/restore CLI/socket smoke ($STORE)"
