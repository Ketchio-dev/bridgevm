#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-snapshot-disk-create.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="snapshot-disk-create-local"
VM_SOCKET="snapshot-disk-create-socket"
SNAPSHOT_NAME="before-disk-create"
QEMU_IMG_LOG="$STORE/qemu-img.log"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >>"${BRIDGEVM_FAKE_QEMU_IMG_LOG:?}"

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
      [[ -f "$backing" ]] || {
        echo "missing backing $backing" >&2
        exit 3
      }
      mkdir -p "$(dirname "$path")"
      printf 'fake snapshot overlay backed by %s\n' "$backing" >"$path"
    else
      path="${@: -2:1}"
      mkdir -p "$(dirname "$path")"
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

for backend in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in snapshot disk-create smoke: $(basename "$0")" >&2
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

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend launch attempted: $(cat "$BACKEND_LOG")"
}

assert_disk_create_contract() {
  local label="$1"
  local vm="$2"
  local runner="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local primary_disk="$bundle/disks/root.qcow2"
  local overlay="$bundle/disks/snapshots/$SNAPSHOT_NAME.qcow2"
  local snapshot_metadata="$bundle/metadata/snapshot-disks/$SNAPSHOT_NAME.json"
  local create_metadata="$bundle/metadata/snapshot-disks/$SNAPSHOT_NAME-create.json"
  local active_disk_metadata="$bundle/metadata/active-disk.json"

  "$runner" create "$vm" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
  "$runner" disk create "$vm" >/dev/null
  "$runner" snapshot create "$vm" "$SNAPSHOT_NAME" --kind disk >/dev/null

  [[ -f "$primary_disk" ]] || fail "$label primary disk was not created"
  [[ ! -e "$overlay" ]] || fail "$label overlay existed before disk-create"
  assert_file_contains "$snapshot_metadata" '"overlay_exists": false' "$label snapshot metadata before"
  assert_file_contains "$snapshot_metadata" '"backing_exists": true' "$label snapshot metadata before"

  output="$("$runner" snapshot disk-create "$vm" "$SNAPSHOT_NAME")"
  assert_contains "$output" "Snapshot disk create executed: true" "$label disk-create output"
  assert_contains "$output" "Snapshot disk create command: qemu-img create -f qcow2 -F qcow2 -b $primary_disk $overlay" "$label disk-create output"
  assert_contains "$output" "Snapshot disk create status:" "$label disk-create output"
  assert_contains "$output" "Snapshot disk create stdout: created $overlay" "$label disk-create output"
  assert_contains "$output" "Snapshot disk overlay: $overlay" "$label disk-create output"
  assert_contains "$output" "Snapshot disk overlay ready: true" "$label disk-create output"
  assert_contains "$output" "Snapshot disk backing: $primary_disk" "$label disk-create output"
  assert_contains "$output" "Snapshot disk backing ready: true" "$label disk-create output"

  [[ -f "$overlay" ]] || fail "$label overlay was not created"
  grep -q "fake snapshot overlay backed by $primary_disk" "$overlay" \
    || fail "$label overlay did not record backing"
  assert_file_contains "$snapshot_metadata" '"overlay_exists": true' "$label snapshot metadata after"
  assert_file_contains "$snapshot_metadata" '"backing_exists": true' "$label snapshot metadata after"
  assert_file_contains "$create_metadata" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label create metadata"
  assert_file_contains "$create_metadata" '"executed": true' "$label create metadata"
  assert_file_contains "$create_metadata" '"exit_status": "exit status: 0"' "$label create metadata"
  assert_file_contains "$create_metadata" "created $overlay" "$label create metadata"
  assert_file_contains "$create_metadata" '"command"' "$label create metadata"
  assert_file_contains "$active_disk_metadata" '"source": "snapshot-overlay"' "$label active disk metadata"
  assert_file_contains "$active_disk_metadata" "\"snapshot\": \"$SNAPSHOT_NAME\"" "$label active disk metadata"
  assert_file_contains "$active_disk_metadata" "\"path\": \"$overlay\"" "$label active disk metadata"
  assert_file_contains "$active_disk_metadata" '"exists": true' "$label active disk metadata"

  chain_output="$("$runner" snapshot chain "$vm")"
  assert_contains "$chain_output" "Active disk source: snapshot-overlay" "$label chain"
  assert_contains "$chain_output" "Active disk snapshot: $SNAPSHOT_NAME" "$label chain"
  assert_contains "$chain_output" "Active disk: $overlay" "$label chain"

  status_output="$("$runner" status "$vm")"
  assert_contains "$status_output" "stopped" "$label status"

  assert_no_backend_launch
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

assert_disk_create_contract \
  "local snapshot disk-create" \
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

assert_disk_create_contract \
  "socket snapshot disk-create" \
  "$VM_SOCKET" \
  bridgevm_socket

assert_no_backend_launch

echo "PASS: snapshot disk-create CLI/socket smoke ($STORE)"
