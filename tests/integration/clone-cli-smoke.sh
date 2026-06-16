#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-clone.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="clone-source"
LOCAL_CLONE_NAME="clone-local-copy"
LOCAL_LINKED_CLONE_NAME="clone-local-linked"
SOCKET_CLONE_NAME="clone-socket-copy"
SOCKET_LINKED_CLONE_NAME="clone-socket-linked"
MISSING_DISK_VM_NAME="clone-missing-disk-source"
LOCAL_MISSING_DISK_CLONE_NAME="clone-local-missing-disk-linked"
SOCKET_MISSING_DISK_CLONE_NAME="clone-socket-missing-disk-linked"
HOST_PATH="$STORE/workspace"
TOKEN="share-token-workspace"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    if [[ " $* " == *" -b "* ]]; then
      path="${@: -1}"
      backing=""
      for ((i = 1; i <= $#; i++)); do
        if [[ "${!i}" == "-b" ]]; then
          next=$((i + 1))
          backing="${!next}"
        fi
      done
      [[ -f "$backing" ]] || {
        echo "missing backing $backing" >&2
        exit 3
      }
      mkdir -p "$(dirname "$path")"
      printf 'fake linked overlay backed by %s\n' "$backing" >"$path"
    else
      path="${@: -2:1}"
      mkdir -p "$(dirname "$path")"
      printf 'fake qcow2\n' >"$path"
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

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

mkdir -p "$HOST_PATH"
bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk create "$VM_NAME" >/dev/null
bridgevm port add "$VM_NAME" 2222:22 >/dev/null
bridgevm share add "$VM_NAME" workspace "$HOST_PATH" --read-only --host-path-token "$TOKEN" >/dev/null
bridgevm snapshot create "$VM_NAME" before-clone --kind disk >/dev/null
bridgevm create "$MISSING_DISK_VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

local_clone_output="$(bridgevm clone "$VM_NAME" "$LOCAL_CLONE_NAME")"
assert_contains "$local_clone_output" "Cloned $LOCAL_CLONE_NAME" "local clone"

local_status="$(bridgevm status "$LOCAL_CLONE_NAME")"
assert_contains "$local_status" "Name: $LOCAL_CLONE_NAME" "local clone status"
assert_contains "$local_status" "State: stopped" "local clone status"

local_snapshots="$(bridgevm snapshot list "$LOCAL_CLONE_NAME")"
assert_contains "$local_snapshots" "before-clone" "local clone snapshots"

local_ports="$(bridgevm port list "$LOCAL_CLONE_NAME")"
assert_contains "$local_ports" "2222:22" "local clone port forwards"

local_shares="$(bridgevm share list "$LOCAL_CLONE_NAME")"
assert_contains "$local_shares" "Shared folder: workspace" "local clone shares"
assert_contains "$local_shares" "$TOKEN" "local clone share token"

LOCAL_MANIFEST="$STORE/vms/$LOCAL_CLONE_NAME.vmbridge/manifest.yaml"
grep -q "name: $LOCAL_CLONE_NAME" "$LOCAL_MANIFEST" \
  || fail "local clone did not rewrite manifest name"
grep -q "hostname: $LOCAL_CLONE_NAME.bridgevm.local" "$LOCAL_MANIFEST" \
  || fail "local clone did not rewrite manifest hostname"
[[ -f "$STORE/vms/$LOCAL_CLONE_NAME.vmbridge/metadata/clone.json" ]] \
  || fail "local clone missing metadata/clone.json"

LOCAL_LINKED_OUTPUT="$(bridgevm clone "$VM_NAME" "$LOCAL_LINKED_CLONE_NAME" --linked)"
assert_contains "$LOCAL_LINKED_OUTPUT" "Cloned $LOCAL_LINKED_CLONE_NAME" "local linked clone"
assert_contains "$LOCAL_LINKED_OUTPUT" "Linked clone: true" "local linked clone"
assert_contains "$LOCAL_LINKED_OUTPUT" "Backing disk: $STORE/vms/$VM_NAME.vmbridge/disks/root.qcow2" "local linked clone"
assert_contains "$LOCAL_LINKED_OUTPUT" "Clone disk create command: qemu-img create -f qcow2 -F qcow2 -b" "local linked clone"

LOCAL_LINKED_BUNDLE="$STORE/vms/$LOCAL_LINKED_CLONE_NAME.vmbridge"
LOCAL_LINKED_MANIFEST="$LOCAL_LINKED_BUNDLE/manifest.yaml"
grep -q "name: $LOCAL_LINKED_CLONE_NAME" "$LOCAL_LINKED_MANIFEST" \
  || fail "local linked clone did not rewrite manifest name"
grep -q "hostname: $LOCAL_LINKED_CLONE_NAME.bridgevm.local" "$LOCAL_LINKED_MANIFEST" \
  || fail "local linked clone did not rewrite manifest hostname"
grep -q "path: disks/root.qcow2" "$LOCAL_LINKED_MANIFEST" \
  || fail "local linked clone did not use overlay manifest disk path"
grep -q "format: qcow2" "$LOCAL_LINKED_MANIFEST" \
  || fail "local linked clone did not use qcow2 overlay format"
[[ -f "$LOCAL_LINKED_BUNDLE/disks/root.qcow2" ]] \
  || fail "local linked clone missing overlay disk"
grep -q "fake linked overlay backed by $STORE/vms/$VM_NAME.vmbridge/disks/root.qcow2" \
  "$LOCAL_LINKED_BUNDLE/disks/root.qcow2" \
  || fail "local linked clone overlay did not record backing"
grep -q '"linked": true' "$LOCAL_LINKED_BUNDLE/metadata/clone.json" \
  || fail "local linked clone metadata did not record linked=true"
grep -q '"backing_path"' "$LOCAL_LINKED_BUNDLE/metadata/clone.json" \
  || fail "local linked clone metadata omitted backing path"
grep -q '"create_command"' "$LOCAL_LINKED_BUNDLE/metadata/clone.json" \
  || fail "local linked clone metadata omitted create command"
[[ "$(tr -d '[:space:]' <"$LOCAL_LINKED_BUNDLE/metadata/snapshots.json")" == "[]" ]] \
  || fail "local linked clone should start with empty snapshots"
[[ ! -d "$LOCAL_LINKED_BUNDLE/metadata/snapshot-disks" ]] \
  || fail "local linked clone should not keep copied snapshot disk metadata"

duplicate_output="$(bridgevm clone "$VM_NAME" "$LOCAL_CLONE_NAME" 2>&1 || true)"
assert_contains "$duplicate_output" "VM already exists" "duplicate local clone"
grep -q "\"vm\": \"$LOCAL_CLONE_NAME\"" "$STORE/vms/$LOCAL_CLONE_NAME.vmbridge/metadata/clone.json" \
  || fail "duplicate local clone disturbed existing clone metadata"

local_missing_disk_output="$(bridgevm clone "$MISSING_DISK_VM_NAME" "$LOCAL_MISSING_DISK_CLONE_NAME" --linked 2>&1 || true)"
assert_contains "$local_missing_disk_output" "primary disk is missing" "local linked clone missing disk"
assert_contains "$local_missing_disk_output" "$STORE/vms/$MISSING_DISK_VM_NAME.vmbridge/disks/root.qcow2" \
  "local linked clone missing disk"
[[ ! -f "$STORE/vms/$LOCAL_MISSING_DISK_CLONE_NAME.vmbridge/metadata/clone.json" ]] \
  || fail "local linked clone missing disk wrote clone metadata"
[[ ! -f "$STORE/vms/$LOCAL_MISSING_DISK_CLONE_NAME.vmbridge/disks/root.qcow2" ]] \
  || fail "local linked clone missing disk created overlay disk"
[[ ! -d "$STORE/vms/$LOCAL_MISSING_DISK_CLONE_NAME.vmbridge" ]] \
  || fail "local linked clone missing disk left partial bundle"

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

socket_clone_output="$(bridgevm_socket clone "$VM_NAME" "$SOCKET_CLONE_NAME")"
assert_contains "$socket_clone_output" "Cloned $SOCKET_CLONE_NAME" "socket clone"

socket_list="$(bridgevm_socket list)"
assert_contains "$socket_list" "$SOCKET_CLONE_NAME" "socket clone list"

socket_status="$(bridgevm_socket status "$SOCKET_CLONE_NAME")"
assert_contains "$socket_status" "$SOCKET_CLONE_NAME" "socket clone status"
assert_contains "$socket_status" "stopped" "socket clone status"

socket_snapshots="$(bridgevm_socket snapshot list "$SOCKET_CLONE_NAME")"
assert_contains "$socket_snapshots" "before-clone" "socket clone snapshots"

socket_ports="$(bridgevm_socket port list "$SOCKET_CLONE_NAME")"
assert_contains "$socket_ports" "2222:22" "socket clone port forwards"

socket_shares="$(bridgevm_socket share list "$SOCKET_CLONE_NAME")"
assert_contains "$socket_shares" "Shared folder: workspace" "socket clone shares"
assert_contains "$socket_shares" "$TOKEN" "socket clone share token"

SOCKET_MANIFEST="$STORE/vms/$SOCKET_CLONE_NAME.vmbridge/manifest.yaml"
grep -q "name: $SOCKET_CLONE_NAME" "$SOCKET_MANIFEST" \
  || fail "socket clone did not rewrite manifest name"
grep -q "hostname: $SOCKET_CLONE_NAME.bridgevm.local" "$SOCKET_MANIFEST" \
  || fail "socket clone did not rewrite manifest hostname"
[[ -f "$STORE/vms/$SOCKET_CLONE_NAME.vmbridge/metadata/clone.json" ]] \
  || fail "socket clone missing metadata/clone.json"

socket_duplicate_output="$(bridgevm_socket clone "$VM_NAME" "$SOCKET_CLONE_NAME" 2>&1 || true)"
assert_contains "$socket_duplicate_output" "VM already exists" "duplicate socket clone"
grep -q "\"vm\": \"$SOCKET_CLONE_NAME\"" "$STORE/vms/$SOCKET_CLONE_NAME.vmbridge/metadata/clone.json" \
  || fail "duplicate socket clone disturbed existing clone metadata"

socket_linked_clone_output="$(bridgevm_socket clone "$VM_NAME" "$SOCKET_LINKED_CLONE_NAME" --linked)"
assert_contains "$socket_linked_clone_output" "Cloned $SOCKET_LINKED_CLONE_NAME" "socket linked clone"
assert_contains "$socket_linked_clone_output" "Linked clone: true" "socket linked clone"
assert_contains "$socket_linked_clone_output" "Backing disk: $STORE/vms/$VM_NAME.vmbridge/disks/root.qcow2" "socket linked clone"

SOCKET_LINKED_BUNDLE="$STORE/vms/$SOCKET_LINKED_CLONE_NAME.vmbridge"
SOCKET_LINKED_MANIFEST="$SOCKET_LINKED_BUNDLE/manifest.yaml"
grep -q "name: $SOCKET_LINKED_CLONE_NAME" "$SOCKET_LINKED_MANIFEST" \
  || fail "socket linked clone did not rewrite manifest name"
grep -q "hostname: $SOCKET_LINKED_CLONE_NAME.bridgevm.local" "$SOCKET_LINKED_MANIFEST" \
  || fail "socket linked clone did not rewrite manifest hostname"
grep -q "path: disks/root.qcow2" "$SOCKET_LINKED_MANIFEST" \
  || fail "socket linked clone did not use overlay manifest disk path"
[[ -f "$SOCKET_LINKED_BUNDLE/disks/root.qcow2" ]] \
  || fail "socket linked clone missing overlay disk"
grep -q "fake linked overlay backed by $STORE/vms/$VM_NAME.vmbridge/disks/root.qcow2" \
  "$SOCKET_LINKED_BUNDLE/disks/root.qcow2" \
  || fail "socket linked clone overlay did not record backing"
grep -q '"linked": true' "$SOCKET_LINKED_BUNDLE/metadata/clone.json" \
  || fail "socket linked clone metadata did not record linked=true"
[[ "$(tr -d '[:space:]' <"$SOCKET_LINKED_BUNDLE/metadata/snapshots.json")" == "[]" ]] \
  || fail "socket linked clone should start with empty snapshots"

socket_missing_disk_output="$(bridgevm_socket clone "$MISSING_DISK_VM_NAME" "$SOCKET_MISSING_DISK_CLONE_NAME" --linked 2>&1 || true)"
assert_contains "$socket_missing_disk_output" "primary disk is missing" "socket linked clone missing disk"
assert_contains "$socket_missing_disk_output" "$STORE/vms/$MISSING_DISK_VM_NAME.vmbridge/disks/root.qcow2" \
  "socket linked clone missing disk"
[[ ! -f "$STORE/vms/$SOCKET_MISSING_DISK_CLONE_NAME.vmbridge/metadata/clone.json" ]] \
  || fail "socket linked clone missing disk wrote clone metadata"
[[ ! -f "$STORE/vms/$SOCKET_MISSING_DISK_CLONE_NAME.vmbridge/disks/root.qcow2" ]] \
  || fail "socket linked clone missing disk created overlay disk"
[[ ! -d "$STORE/vms/$SOCKET_MISSING_DISK_CLONE_NAME.vmbridge" ]] \
  || fail "socket linked clone missing disk left partial bundle"

echo "PASS: clone CLI/socket integration smoke ($STORE)"
