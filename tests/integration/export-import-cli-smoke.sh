#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SOURCE_STORE="$(mktemp -d "/tmp/bridgevm-export-source.XXXXXX")"
TARGET_STORE="$(mktemp -d "/tmp/bridgevm-import-target.XXXXXX")"
SOCKET_STORE="$(mktemp -d "/tmp/bridgevm-import-socket.XXXXXX")"
VM_NAME="portable-linux"
LOCAL_IMPORT_NAME="portable-copy"
LOCAL_TAR_IMPORT_NAME="portable-tar-copy"
SOCKET_IMPORT_NAME="portable-socket"
SOCKET_TAR_IMPORT_NAME="portable-tar-socket"
EXPORT_BUNDLE="$SOURCE_STORE/exports/$VM_NAME.vmbridge"
EXPORT_TAR="$SOURCE_STORE/exports/$VM_NAME.tar"
MALFORMED_TAR="$SOURCE_STORE/exports/malformed.tar"
TRAVERSAL_TAR="$SOURCE_STORE/exports/traversal.tar"
DAEMON_PID=""

bridgevm_source() {
  cargo run --quiet -p bridgevm-cli -- --store "$SOURCE_STORE" "$@"
}

bridgevm_target() {
  cargo run --quiet -p bridgevm-cli -- --store "$TARGET_STORE" "$@"
}

bridgevmd_socket_store() {
  cargo run --quiet -p bridgevm-daemon -- --store "$SOCKET_STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Source store preserved at $SOURCE_STORE" >&2
  echo "Target store preserved at $TARGET_STORE" >&2
  echo "Socket store preserved at $SOCKET_STORE" >&2
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
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  grep -Fq -- "$needle" "$file" || fail "$label missing '$needle' in $file"
}

assert_no_live_artifacts() {
  local path="$1"
  local label="$2"
  if find "$path" \( -name "*.sock" -o -name "*.lock" \) -print -quit | grep -q .; then
    fail "$label included socket or lock paths"
  fi
}

assert_tar_contains_entry() {
  local archive="$1"
  local entry="$2"
  local label="$3"
  if ! tar -tf "$archive" | grep -Fx -- "$entry" >/dev/null; then
    fail "$label missing tar entry $entry"
  fi
}

assert_tar_safe_paths() {
  local archive="$1"
  local label="$2"
  if tar -tf "$archive" | grep -E '(^/|(^|/)\.\.(/|$))' >/dev/null; then
    fail "$label included absolute or traversal tar paths"
  fi
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm_source create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm_source port add "$VM_NAME" 2222:22 >/dev/null
mkdir -p "$SOURCE_STORE/workspace"
bridgevm_source share add "$VM_NAME" workspace "$SOURCE_STORE/workspace" --host-path-token share-token-workspace >/dev/null
bridgevm_source snapshot create "$VM_NAME" before-export --kind disk >/dev/null
mkdir -p "$SOURCE_STORE/vms/$VM_NAME.vmbridge/metadata"
printf "socket placeholder\n" >"$SOURCE_STORE/vms/$VM_NAME.vmbridge/metadata/qmp.sock"
printf "locked\n" >"$SOURCE_STORE/vms/$VM_NAME.vmbridge/metadata/export.lock"

bridgevm_source export "$VM_NAME" --output "$EXPORT_BUNDLE" >/dev/null
bridgevm_source export "$VM_NAME" --output "$EXPORT_TAR" >/dev/null
printf "not a bridgevm tar archive\n" >"$MALFORMED_TAR"
python3 - "$TRAVERSAL_TAR" <<'PY'
import io
import sys
import tarfile

with tarfile.open(sys.argv[1], "w") as archive:
    payload = b"name: traversal\n"
    entry = tarfile.TarInfo("../manifest.yaml")
    entry.size = len(payload)
    archive.addfile(entry, io.BytesIO(payload))
PY

[[ -f "$EXPORT_BUNDLE/manifest.yaml" ]] || fail "export missing manifest.yaml"
[[ -f "$EXPORT_BUNDLE/metadata/export.json" ]] || fail "export missing metadata/export.json"
[[ -f "$EXPORT_BUNDLE/metadata/snapshots.json" ]] || fail "export missing metadata/snapshots.json"
[[ -f "$EXPORT_TAR" ]] || fail "tar export missing archive"
assert_no_live_artifacts "$EXPORT_BUNDLE" "directory export"
assert_file_contains "$EXPORT_BUNDLE/metadata/export.json" '"archive_format": "directory"' "directory export metadata"
assert_file_contains "$EXPORT_BUNDLE/metadata/export.json" '"manifest_preserved": true' "directory export metadata"
assert_file_contains "$EXPORT_BUNDLE/metadata/export.json" '"metadata_preserved": true' "directory export metadata"
if tar -tf "$EXPORT_TAR" | grep -E '(\.sock|\.lock)$' >/dev/null; then
  fail "tar export included socket or lock paths"
fi
assert_tar_safe_paths "$EXPORT_TAR" "tar export"
assert_tar_contains_entry "$EXPORT_TAR" "manifest.yaml" "tar export"
assert_tar_contains_entry "$EXPORT_TAR" "metadata/export.json" "tar export"
assert_tar_contains_entry "$EXPORT_TAR" "metadata/snapshots.json" "tar export"

inside_source_output="$(
  bridgevm_source export "$VM_NAME" \
    --output "$SOURCE_STORE/vms/$VM_NAME.vmbridge/nested-export.vmbridge" 2>&1 || true
)"
assert_contains "$inside_source_output" "export output must not be the source bundle or inside it" \
  "export inside source"
[[ ! -e "$SOURCE_STORE/vms/$VM_NAME.vmbridge/nested-export.vmbridge" ]] \
  || fail "export inside source created a nested bundle"

local_import_output="$(bridgevm_target import "$EXPORT_BUNDLE" --name "$LOCAL_IMPORT_NAME")"
assert_contains "$local_import_output" "Imported $LOCAL_IMPORT_NAME" "local import"

local_status="$(bridgevm_target status "$LOCAL_IMPORT_NAME")"
assert_contains "$local_status" "Name: $LOCAL_IMPORT_NAME" "local imported status"
assert_contains "$local_status" "State: stopped" "local imported status"

local_snapshots="$(bridgevm_target snapshot list "$LOCAL_IMPORT_NAME")"
assert_contains "$local_snapshots" "before-export" "local imported snapshots"

local_ports="$(bridgevm_target port list "$LOCAL_IMPORT_NAME")"
assert_contains "$local_ports" "2222:22" "local imported port forwards"

local_shares="$(bridgevm_target share list "$LOCAL_IMPORT_NAME")"
assert_contains "$local_shares" "Shared folder: workspace" "local imported shares"
assert_contains "$local_shares" "share-token-workspace" "local imported share token"

LOCAL_IMPORT_BUNDLE="$TARGET_STORE/vms/$LOCAL_IMPORT_NAME.vmbridge"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/manifest.yaml" "name: $LOCAL_IMPORT_NAME" "local import manifest"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/manifest.yaml" "hostname: $LOCAL_IMPORT_NAME.bridgevm.local" "local import manifest"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/manifest.yaml" "2222" "local import manifest port metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/manifest.yaml" "share-token-workspace" "local import manifest share metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/snapshots.json" "before-export" "local import snapshot metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/export.json" '"archive_format": "directory"' "local import preserved export metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" "\"vm\": \"$LOCAL_IMPORT_NAME\"" "local import metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" "\"original_name\": \"$VM_NAME\"" "local import metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" '"archive_format": "directory"' "local import metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" '"manifest_preserved": true' "local import metadata"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" '"metadata_preserved": true' "local import metadata"
assert_no_live_artifacts "$LOCAL_IMPORT_BUNDLE" "local directory import"

duplicate_output="$(bridgevm_target import "$EXPORT_BUNDLE" --name "$LOCAL_IMPORT_NAME" 2>&1 || true)"
assert_contains "$duplicate_output" "VM already exists" "duplicate local import"
assert_file_contains "$LOCAL_IMPORT_BUNDLE/metadata/import.json" "\"vm\": \"$LOCAL_IMPORT_NAME\"" \
  "duplicate local import preserved existing metadata"

malformed_output="$(bridgevm_target import "$MALFORMED_TAR" --name malformed-copy 2>&1 || true)"
assert_contains "$malformed_output" "failed to import VM bundle" "malformed tar import"
[[ ! -e "$TARGET_STORE/vms/malformed-copy.vmbridge" ]] \
  || fail "malformed tar import created a VM bundle"

traversal_output="$(bridgevm_target import "$TRAVERSAL_TAR" --name traversal-copy 2>&1 || true)"
assert_contains "$traversal_output" "unsafe path" "path traversal tar import"
[[ ! -e "$TARGET_STORE/vms/traversal-copy.vmbridge" ]] \
  || fail "path traversal tar import created a VM bundle"

local_tar_import_output="$(bridgevm_target import "$EXPORT_TAR" --name "$LOCAL_TAR_IMPORT_NAME")"
assert_contains "$local_tar_import_output" "Imported $LOCAL_TAR_IMPORT_NAME" "local tar import"

local_tar_status="$(bridgevm_target status "$LOCAL_TAR_IMPORT_NAME")"
assert_contains "$local_tar_status" "Name: $LOCAL_TAR_IMPORT_NAME" "local tar imported status"
assert_contains "$local_tar_status" "State: stopped" "local tar imported status"

local_tar_snapshots="$(bridgevm_target snapshot list "$LOCAL_TAR_IMPORT_NAME")"
assert_contains "$local_tar_snapshots" "before-export" "local tar imported snapshots"

local_tar_ports="$(bridgevm_target port list "$LOCAL_TAR_IMPORT_NAME")"
assert_contains "$local_tar_ports" "2222:22" "local tar imported port forwards"

local_tar_shares="$(bridgevm_target share list "$LOCAL_TAR_IMPORT_NAME")"
assert_contains "$local_tar_shares" "Shared folder: workspace" "local tar imported shares"

LOCAL_TAR_IMPORT_BUNDLE="$TARGET_STORE/vms/$LOCAL_TAR_IMPORT_NAME.vmbridge"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/manifest.yaml" "name: $LOCAL_TAR_IMPORT_NAME" "local tar import manifest"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/manifest.yaml" "2222" "local tar import manifest port metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/manifest.yaml" "share-token-workspace" "local tar import manifest share metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/metadata/snapshots.json" "before-export" "local tar import snapshot metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/metadata/export.json" '"archive_format": "tar"' "local tar import preserved export metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/metadata/import.json" '"archive_format": "tar"' "local tar import metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/metadata/import.json" '"manifest_preserved": true' "local tar import metadata"
assert_file_contains "$LOCAL_TAR_IMPORT_BUNDLE/metadata/import.json" '"metadata_preserved": true' "local tar import metadata"
assert_no_live_artifacts "$LOCAL_TAR_IMPORT_BUNDLE" "local tar import"

SOCKET="$SOCKET_STORE/run/bridgevmd.sock"
DAEMON_LOG="$SOCKET_STORE/bridgevmd.log"

bridgevmd_socket_store >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

socket_import_output="$(bridgevm_socket import "$EXPORT_BUNDLE" --name "$SOCKET_IMPORT_NAME")"
assert_contains "$socket_import_output" "Imported $SOCKET_IMPORT_NAME" "socket import"

socket_list="$(bridgevm_socket list)"
assert_contains "$socket_list" "$SOCKET_IMPORT_NAME" "socket imported list"

socket_snapshots="$(bridgevm_socket snapshot list "$SOCKET_IMPORT_NAME")"
assert_contains "$socket_snapshots" "before-export" "socket imported snapshots"

socket_status="$(bridgevm_socket status "$SOCKET_IMPORT_NAME")"
assert_contains "$socket_status" "$SOCKET_IMPORT_NAME" "socket imported status"
assert_contains "$socket_status" "stopped" "socket imported status"

socket_ports="$(bridgevm_socket port list "$SOCKET_IMPORT_NAME")"
assert_contains "$socket_ports" "2222:22" "socket imported port forwards"

socket_shares="$(bridgevm_socket share list "$SOCKET_IMPORT_NAME")"
assert_contains "$socket_shares" "Shared folder: workspace" "socket imported shares"
assert_contains "$socket_shares" "share-token-workspace" "socket imported share token"

SOCKET_IMPORT_BUNDLE="$SOCKET_STORE/vms/$SOCKET_IMPORT_NAME.vmbridge"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/manifest.yaml" "name: $SOCKET_IMPORT_NAME" "socket import manifest"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/manifest.yaml" "hostname: $SOCKET_IMPORT_NAME.bridgevm.local" "socket import manifest"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/manifest.yaml" "2222" "socket import manifest port metadata"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/manifest.yaml" "share-token-workspace" "socket import manifest share metadata"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/metadata/snapshots.json" "before-export" "socket import snapshot metadata"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/metadata/import.json" '"archive_format": "directory"' "socket import metadata"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/metadata/import.json" '"metadata_preserved": true' "socket import metadata"
assert_no_live_artifacts "$SOCKET_IMPORT_BUNDLE" "socket directory import"

socket_duplicate_output="$(bridgevm_socket import "$EXPORT_BUNDLE" --name "$SOCKET_IMPORT_NAME" 2>&1 || true)"
assert_contains "$socket_duplicate_output" "VM already exists" "duplicate socket import"
assert_file_contains "$SOCKET_IMPORT_BUNDLE/metadata/import.json" "\"vm\": \"$SOCKET_IMPORT_NAME\"" \
  "duplicate socket import preserved existing metadata"

socket_tar_import_output="$(bridgevm_socket import "$EXPORT_TAR" --name "$SOCKET_TAR_IMPORT_NAME")"
assert_contains "$socket_tar_import_output" "Imported $SOCKET_TAR_IMPORT_NAME" "socket tar import"

socket_tar_status="$(bridgevm_socket status "$SOCKET_TAR_IMPORT_NAME")"
assert_contains "$socket_tar_status" "$SOCKET_TAR_IMPORT_NAME" "socket tar imported status"
assert_contains "$socket_tar_status" "stopped" "socket tar imported status"

socket_tar_snapshots="$(bridgevm_socket snapshot list "$SOCKET_TAR_IMPORT_NAME")"
assert_contains "$socket_tar_snapshots" "before-export" "socket tar imported snapshots"

socket_tar_ports="$(bridgevm_socket port list "$SOCKET_TAR_IMPORT_NAME")"
assert_contains "$socket_tar_ports" "2222:22" "socket tar imported port forwards"

socket_tar_shares="$(bridgevm_socket share list "$SOCKET_TAR_IMPORT_NAME")"
assert_contains "$socket_tar_shares" "Shared folder: workspace" "socket tar imported shares"
assert_contains "$socket_tar_shares" "share-token-workspace" "socket tar imported share token"

SOCKET_TAR_IMPORT_BUNDLE="$SOCKET_STORE/vms/$SOCKET_TAR_IMPORT_NAME.vmbridge"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/manifest.yaml" "name: $SOCKET_TAR_IMPORT_NAME" "socket tar import manifest"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/manifest.yaml" "2222" "socket tar import manifest port metadata"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/manifest.yaml" "share-token-workspace" "socket tar import manifest share metadata"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/metadata/snapshots.json" "before-export" "socket tar import snapshot metadata"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/metadata/import.json" '"archive_format": "tar"' "socket tar import metadata"
assert_file_contains "$SOCKET_TAR_IMPORT_BUNDLE/metadata/import.json" '"metadata_preserved": true' "socket tar import metadata"
assert_no_live_artifacts "$SOCKET_TAR_IMPORT_BUNDLE" "socket tar import"

echo "PASS: export/import CLI/socket integration smoke ($SOURCE_STORE -> $TARGET_STORE, $SOCKET_STORE)"
