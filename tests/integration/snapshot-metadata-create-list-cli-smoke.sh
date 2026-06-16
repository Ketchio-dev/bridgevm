#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-snapshot-metadata-create-list.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="snapshot-metadata-local"
VM_SOCKET="snapshot-metadata-socket"
SNAPSHOT_NAME="metadata-only"
QEMU_IMG_LOG="$STORE/qemu-img.log"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >>"${BRIDGEVM_FAKE_QEMU_IMG_LOG:?}"
echo "qemu-img execution is forbidden in snapshot metadata create/list smoke" >&2
exit 99
SH
chmod +x "$FAKE_BIN/qemu-img"

for backend in qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in snapshot metadata create/list smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_QEMU_IMG_LOG="$QEMU_IMG_LOG"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"

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

assert_no_host_tool_execution() {
  [[ ! -s "$QEMU_IMG_LOG" ]] || fail "qemu-img was executed: $(cat "$QEMU_IMG_LOG")"
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend launch attempted: $(cat "$BACKEND_LOG")"
}

assert_snapshot_metadata_create_list_contract() {
  local label="$1"
  local vm="$2"
  local runner="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local primary_disk="$bundle/disks/root.qcow2"
  local overlay="$bundle/disks/snapshots/$SNAPSHOT_NAME.qcow2"
  local snapshots_metadata="$bundle/metadata/snapshots.json"
  local snapshot_disk_metadata="$bundle/metadata/snapshot-disks/$SNAPSHOT_NAME.json"
  local disk_create_metadata="$bundle/metadata/snapshot-disks/$SNAPSHOT_NAME-create.json"
  local last_restore="$bundle/metadata/last-restore.json"

  "$runner" create "$vm" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
  "$runner" start "$vm" >/dev/null

  create_output="$("$runner" snapshot create "$vm" "$SNAPSHOT_NAME" --kind disk)"
  assert_contains "$create_output" "Created disk snapshot '$SNAPSHOT_NAME'" "$label create"
  assert_contains "$create_output" "Snapshot disk overlay: $overlay" "$label create"
  assert_contains "$create_output" "Snapshot disk overlay ready: false" "$label create"
  assert_contains "$create_output" "Snapshot disk backing: $primary_disk" "$label create"
  assert_contains "$create_output" "Snapshot disk backing ready: false" "$label create"

  [[ ! -e "$primary_disk" ]] || fail "$label created a primary disk"
  [[ ! -e "$overlay" ]] || fail "$label created a snapshot overlay"
  [[ ! -e "$disk_create_metadata" ]] || fail "$label wrote disk-create execution metadata"
  [[ ! -e "$last_restore" ]] || fail "$label wrote restore metadata"

  assert_file_contains "$snapshots_metadata" "\"name\": \"$SNAPSHOT_NAME\"" "$label snapshots metadata"
  assert_file_contains "$snapshots_metadata" '"kind": "disk"' "$label snapshots metadata"
  assert_file_contains "$snapshots_metadata" '"vm_state": "running"' "$label snapshots metadata"
  assert_file_contains "$snapshots_metadata" '"created_at_unix":' "$label snapshots metadata"
  assert_file_contains "$snapshot_disk_metadata" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label disk metadata"
  assert_file_contains "$snapshot_disk_metadata" '"overlay_exists": false' "$label disk metadata"
  assert_file_contains "$snapshot_disk_metadata" '"backing_exists": false' "$label disk metadata"
  assert_file_contains "$snapshot_disk_metadata" "\"overlay_path\": \"$overlay\"" "$label disk metadata"
  assert_file_contains "$snapshot_disk_metadata" "\"backing_path\": \"$primary_disk\"" "$label disk metadata"
  assert_file_contains "$snapshot_disk_metadata" '"create_command":' "$label disk metadata"

  list_output="$("$runner" snapshot list "$vm")"
  assert_contains "$list_output" "$SNAPSHOT_NAME" "$label list"
  assert_contains "$list_output" "disk" "$label list"
  assert_contains "$list_output" "running" "$label list"

  chain_output="$("$runner" snapshot chain "$vm")"
  assert_contains "$chain_output" "Active disk source: primary" "$label chain"
  assert_contains "$chain_output" "Snapshot disk: $SNAPSHOT_NAME" "$label chain"
  assert_contains "$chain_output" "Snapshot disk overlay ready: false" "$label chain"
  assert_contains "$chain_output" "Snapshot disk backing ready: false" "$label chain"

  assert_fails_contains \
    "$label duplicate snapshot" \
    "snapshot already exists for $vm: $SNAPSHOT_NAME" \
    "$runner" snapshot create "$vm" "$SNAPSHOT_NAME" --kind disk

  assert_no_host_tool_execution
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

assert_snapshot_metadata_create_list_contract \
  "local snapshot metadata create/list" \
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

assert_snapshot_metadata_create_list_contract \
  "socket snapshot metadata create/list" \
  "$VM_SOCKET" \
  bridgevm_socket

assert_no_host_tool_execution

echo "PASS: snapshot metadata create/list CLI/socket smoke ($STORE)"
