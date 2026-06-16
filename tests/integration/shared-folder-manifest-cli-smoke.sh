#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-share-manifest.XXXXXX")"
VM_NAME="share-linux"
MANIFEST="$STORE/vms/$VM_NAME.vmbridge/manifest.yaml"
HOST_PATH="$STORE/workspace"
DOWNLOADS_PATH="$STORE/downloads"
APPROVED_PATH="$STORE/approved"
OUTSIDE_PATH="$STORE/outside"
SYMLINK_PATH="$APPROVED_PATH/link-out"
TOKEN="share-token-workspace"

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

assert_share_add_rejected() {
  local label="$1"
  local runner="$2"
  local expected="$3"
  shift 3

  local output="$STORE/$label.out"
  if "$runner" share add "$@" >"$output" 2>&1; then
    fail "$label unexpectedly succeeded"
  fi
  grep -q "$expected" "$output" \
    || fail "$label did not report '$expected': $(cat "$output")"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

mkdir -p "$HOST_PATH" "$DOWNLOADS_PATH" "$APPROVED_PATH" "$OUTSIDE_PATH"
HOST_PATH="$(cd "$HOST_PATH" && pwd -P)"
DOWNLOADS_PATH="$(cd "$DOWNLOADS_PATH" && pwd -P)"
APPROVED_PATH="$(cd "$APPROVED_PATH" && pwd -P)"
OUTSIDE_PATH="$(cd "$OUTSIDE_PATH" && pwd -P)"
SYMLINK_PATH="$APPROVED_PATH/link-out"
ln -s "$OUTSIDE_PATH" "$SYMLINK_PATH"
bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode fast >/dev/null

initial_shares="$(bridgevm share list "$VM_NAME")"
case "$initial_shares" in
  *"No shared folders configured"*) ;;
  *) fail "new VM should not have shared folders, got: $initial_shares" ;;
esac

assert_share_add_rejected "local-empty-share-name" bridgevm \
  "shared folder 0 field name cannot be empty" \
  "$VM_NAME" "" "$HOST_PATH"
assert_share_add_rejected "local-whitespace-share-name" bridgevm \
  "shared folder 0 field name cannot be empty" \
  "$VM_NAME" "   " "$HOST_PATH"
assert_share_add_rejected "local-empty-host-path" bridgevm \
  "shared folder 0 field hostPath cannot be empty" \
  "$VM_NAME" empty-host ""
assert_share_add_rejected "local-whitespace-host-path" bridgevm \
  "shared folder 0 field hostPath cannot be empty" \
  "$VM_NAME" whitespace-host "   "
assert_share_add_rejected "local-empty-host-path-token" bridgevm \
  "shared folder 0 field hostPathToken cannot be empty" \
  "$VM_NAME" empty-token "$HOST_PATH" --host-path-token ""
assert_share_add_rejected "local-whitespace-host-path-token" bridgevm \
  "shared folder 0 field hostPathToken cannot be empty" \
  "$VM_NAME" whitespace-token "$HOST_PATH" --host-path-token "   "
assert_share_add_rejected "local-relative-host-path" bridgevm \
  "shared folder hostPath must be an absolute path" \
  "$VM_NAME" relative-host "relative/path"
assert_share_add_rejected "local-parent-host-path" bridgevm \
  "shared folder hostPath cannot contain '..' components" \
  "$VM_NAME" parent-host "$APPROVED_PATH/../outside"
assert_share_add_rejected "local-symlink-host-path" bridgevm \
  "cannot traverse symlink" \
  "$VM_NAME" symlink-host "$SYMLINK_PATH"

bridgevm share add "$VM_NAME" workspace "$HOST_PATH" --read-only --host-path-token "$TOKEN" >/dev/null

grep -q "name: workspace" "$MANIFEST" || fail "manifest did not record shared folder name"
grep -q "hostPath: $HOST_PATH" "$MANIFEST" || fail "manifest did not record host path"
grep -q "readOnly: true" "$MANIFEST" || fail "manifest did not record read-only flag"
grep -q "hostPathToken: $TOKEN" "$MANIFEST" || fail "manifest did not record host path token"

shares_after_add="$(bridgevm share list "$VM_NAME")"
case "$shares_after_add" in
  *"Shared folder: workspace"*"$HOST_PATH"*"$TOKEN"*) ;;
  *) fail "share list did not show added folder, got: $shares_after_add" ;;
esac

if bridgevm share add "$VM_NAME" workspace "$STORE/other" >"$STORE/duplicate-name.out" 2>&1; then
  fail "duplicate shared folder add should fail"
fi
grep -q "duplicate shared folder name 'workspace'" "$STORE/duplicate-name.out" \
  || fail "duplicate add did not report duplicate share name"

if bridgevm share add "$VM_NAME" downloads "$DOWNLOADS_PATH" --host-path-token "$TOKEN" >"$STORE/duplicate-token.out" 2>&1; then
  fail "duplicate shared folder token should fail"
fi
grep -q "duplicate shared folder token '$TOKEN'" "$STORE/duplicate-token.out" \
  || fail "duplicate token add did not report duplicate share token"

bridgevm share remove "$VM_NAME" workspace >/dev/null

shares_after_remove="$(bridgevm share list "$VM_NAME")"
case "$shares_after_remove" in
  *"No shared folders configured"*) ;;
  *) fail "share list still showed folders after remove, got: $shares_after_remove" ;;
esac

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

socket_initial_shares="$(bridgevm_socket share list "$VM_NAME")"
case "$socket_initial_shares" in
  *"No shared folders configured"*) ;;
  *) fail "socket share list should start empty, got: $socket_initial_shares" ;;
esac

assert_share_add_rejected "socket-empty-share-name" bridgevm_socket \
  "shared folder 0 field name cannot be empty" \
  "$VM_NAME" "" "$HOST_PATH"
assert_share_add_rejected "socket-whitespace-share-name" bridgevm_socket \
  "shared folder 0 field name cannot be empty" \
  "$VM_NAME" "   " "$HOST_PATH"
assert_share_add_rejected "socket-empty-host-path" bridgevm_socket \
  "shared folder 0 field hostPath cannot be empty" \
  "$VM_NAME" empty-host ""
assert_share_add_rejected "socket-whitespace-host-path" bridgevm_socket \
  "shared folder 0 field hostPath cannot be empty" \
  "$VM_NAME" whitespace-host "   "
assert_share_add_rejected "socket-empty-host-path-token" bridgevm_socket \
  "shared folder 0 field hostPathToken cannot be empty" \
  "$VM_NAME" empty-token "$HOST_PATH" --host-path-token ""
assert_share_add_rejected "socket-whitespace-host-path-token" bridgevm_socket \
  "shared folder 0 field hostPathToken cannot be empty" \
  "$VM_NAME" whitespace-token "$HOST_PATH" --host-path-token "   "
assert_share_add_rejected "socket-relative-host-path" bridgevm_socket \
  "shared folder hostPath must be an absolute path" \
  "$VM_NAME" relative-host "relative/path"
assert_share_add_rejected "socket-parent-host-path" bridgevm_socket \
  "shared folder hostPath cannot contain '..' components" \
  "$VM_NAME" parent-host "$APPROVED_PATH/../outside"
assert_share_add_rejected "socket-symlink-host-path" bridgevm_socket \
  "cannot traverse symlink" \
  "$VM_NAME" symlink-host "$SYMLINK_PATH"

bridgevm_socket share add "$VM_NAME" workspace "$HOST_PATH" --read-only --host-path-token "$TOKEN" >/dev/null

socket_shares_after_add="$(bridgevm_socket share list "$VM_NAME")"
case "$socket_shares_after_add" in
  *"Shared folder: workspace"*"$HOST_PATH"*"Read-only: true"*"$TOKEN"*) ;;
  *) fail "socket share list did not show added folder, got: $socket_shares_after_add" ;;
esac

assert_share_add_rejected "socket-duplicate-name" bridgevm_socket \
  "duplicate shared folder name 'workspace'" \
  "$VM_NAME" workspace "$STORE/other"
assert_share_add_rejected "socket-duplicate-token" bridgevm_socket \
  "duplicate shared folder token '$TOKEN'" \
  "$VM_NAME" downloads "$DOWNLOADS_PATH" --host-path-token "$TOKEN"

approved_status="$(bridgevm_socket guest-tools status "$VM_NAME")"
case "$approved_status" in
  *"Approved shared folder: workspace"*"$TOKEN"*"Approved shared folder read-only: true"*) ;;
  *) fail "guest-tools status did not expose approved share, got: $approved_status" ;;
esac

bridgevm_socket share remove "$VM_NAME" workspace >/dev/null

socket_shares_after_remove="$(bridgevm_socket share list "$VM_NAME")"
case "$socket_shares_after_remove" in
  *"No shared folders configured"*) ;;
  *) fail "socket share list still showed folders after remove, got: $socket_shares_after_remove" ;;
esac

echo "PASS: shared-folder manifest CLI/socket integration smoke ($STORE)"
