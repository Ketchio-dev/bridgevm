#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-suspend-snapshot.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="suspend-snapshot-local"
VM_SOCKET="suspend-snapshot-socket"
SNAPSHOT_NAME="paused"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

for backend in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in suspend snapshot smoke: $(basename "$0")" >&2
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

write_state_fixture() {
  local vm="$1"
  local state="$2"
  local metadata_dir="$STORE/vms/$vm.vmbridge/metadata"
  mkdir -p "$metadata_dir"
  cat >"$metadata_dir/state.json" <<EOF
{
  "state": "$state",
  "updated_at_unix": 1
}
EOF
}

assert_suspend_snapshot_contract() {
  local label="$1"
  local vm="$2"
  local create_output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local metadata="$bundle/metadata/suspend-images/$SNAPSHOT_NAME.json"
  local image="$bundle/suspend-images/$SNAPSHOT_NAME.bin"
  local last_restore="$bundle/metadata/last-restore.json"

  assert_contains "$create_output" "Created suspend snapshot '$SNAPSHOT_NAME'" "$label create"
  assert_file_contains "$metadata" '"image_format": "bridgevm-suspend-image-v1"' "$label metadata"
  assert_file_contains "$metadata" '"image_exists": false' "$label metadata"
  assert_file_contains "$metadata" "suspend-images/$SNAPSHOT_NAME.bin" "$label metadata"
  [[ ! -e "$image" ]] || fail "$label create unexpectedly wrote suspend image: $image"

  missing_restore="$("$RUNNER" snapshot restore "$vm" "$SNAPSHOT_NAME" 2>&1 || true)"
  assert_contains "$missing_restore" "suspend image is missing" "$label missing restore"
  [[ ! -e "$last_restore" ]] || fail "$label missing-image restore wrote last-restore metadata"

  mkdir -p "$(dirname "$image")"
  printf 'fake suspend image for %s\n' "$vm" >"$image"

  restore_output="$("$RUNNER" snapshot restore "$vm" "$SNAPSHOT_NAME")"
  assert_contains "$restore_output" "Restored snapshot '$SNAPSHOT_NAME' metadata" "$label restore"
  assert_contains "$restore_output" "recorded state: suspended" "$label restore"
  assert_contains "$restore_output" "Suspend image: $image" "$label restore"
  assert_contains "$restore_output" "Suspend image format: bridgevm-suspend-image-v1" "$label restore"
  assert_contains "$restore_output" "Suspend image ready: true" "$label restore"

  assert_file_contains "$metadata" '"image_exists": true' "$label restored metadata"
  assert_file_contains "$last_restore" '"suspend_image"' "$label last restore"
  assert_file_contains "$last_restore" '"restored_state": "suspended"' "$label last restore"

  status_output="$("$RUNNER" status "$vm")"
  assert_contains "$status_output" "suspended" "$label restored status"
  assert_no_backend_launch
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch arm64 --mode fast >/dev/null
bridgevm start "$VM_LOCAL" >/dev/null
write_state_fixture "$VM_LOCAL" "suspended"
local_create="$(bridgevm snapshot create "$VM_LOCAL" "$SNAPSHOT_NAME" --kind suspend)"
write_state_fixture "$VM_LOCAL" "running"

RUNNER=bridgevm
assert_suspend_snapshot_contract "local suspend snapshot" "$VM_LOCAL" "$local_create"

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

bridgevm create "$VM_SOCKET" --os ubuntu --arch arm64 --mode fast >/dev/null
bridgevm_socket start "$VM_SOCKET" >/dev/null
write_state_fixture "$VM_SOCKET" "suspended"
socket_create="$(bridgevm_socket snapshot create "$VM_SOCKET" "$SNAPSHOT_NAME" --kind suspend)"
write_state_fixture "$VM_SOCKET" "running"

RUNNER=bridgevm_socket
assert_suspend_snapshot_contract "socket suspend snapshot" "$VM_SOCKET" "$socket_create"

echo "PASS: suspend snapshot CLI/socket smoke ($STORE)"
