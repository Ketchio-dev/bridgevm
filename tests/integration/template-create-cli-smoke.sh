#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-template-create.XXXXXX")"
VM_LOCAL="template-local"
VM_SOCKET="template-socket"
TEMPLATE_ID="ubuntu-arm64-installer"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
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
  if [[ -f "$DAEMON_LOG" ]]; then
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

assert_fails_contains() {
  local label="$1"
  local expected="$2"
  shift 2

  local stdout="$STORE/$label.stdout"
  local stderr="$STORE/$label.stderr"
  if "$@" >"$stdout" 2>"$stderr"; then
    fail "$label unexpectedly succeeded"
  fi

  local output
  output="$(cat "$stdout" "$stderr")"
  ASSERT_OUTPUT="$output"
  assert_contains "$output" "$expected" "$label"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

assert_template_listing() {
  local label="$1"
  local output="$2"

  assert_contains "$output" "Boot template id: $TEMPLATE_ID" "$label"
  assert_contains "$output" "Guest: ubuntu arm64" "$label"
  assert_contains "$output" "Boot template: linux-installer" "$label"
  assert_contains "$output" "Boot media: ubuntu arm64 installer image" "$label"
  assert_contains "$output" "Installer image: installers/ubuntu-arm64.iso" "$label"
  assert_contains "$output" "Boot template id: fedora-arm64-installer" "$label"
  assert_contains "$output" "Installer image: installers/fedora-arm64.iso" "$label"
  assert_contains "$output" "Boot template id: debian-arm64-installer" "$label"
  assert_contains "$output" "Installer image: installers/debian-arm64.iso" "$label"
  assert_contains "$output" "Boot template id: macos-restore" "$label"
  assert_contains "$output" "Boot template: macos-restore" "$label"
  assert_contains "$output" "macOS restore image: installers/macos-restore.ipsw" "$label"
}

assert_template_manifest() {
  local label="$1"
  local vm="$2"
  local guest_os="$3"
  local boot_mode="$4"
  local media_key="$5"
  local media_path="$6"
  local manifest="$STORE/vms/$vm.vmbridge/manifest.yaml"

  [[ -f "$manifest" ]] || fail "$label manifest missing: $manifest"
  grep -q "name: $vm" "$manifest" || fail "$label manifest omitted name"
  grep -q "mode: fast" "$manifest" || fail "$label manifest omitted fast mode"
  grep -q "os: $guest_os" "$manifest" || fail "$label manifest omitted guest os"
  grep -q "arch: arm64" "$manifest" || fail "$label manifest omitted guest arch"
  grep -q "mode: $boot_mode" "$manifest" || fail "$label manifest omitted boot mode"
  grep -q "$media_key: $media_path" "$manifest" \
    || fail "$label manifest omitted boot media path"
}

assert_template_create() {
  local label="$1"
  local vm="$2"
  local template_id="$3"
  local guest_os="$4"
  local boot_mode="$5"
  local media_key="$6"
  local media_path="$7"
  shift 7

  local output
  output="$("$@" create "$vm" --template "$template_id")"
  assert_contains "$output" "$vm" "$label create"
  assert_contains "$output" "fast" "$label create"
  assert_template_manifest "$label create" "$vm" "$guest_os" "$boot_mode" "$media_key" "$media_path"
}

trap stop_daemon EXIT

local_templates="$(bridgevm templates)"
assert_template_listing "local templates" "$local_templates"

local_create="$(bridgevm create "$VM_LOCAL" --template "$TEMPLATE_ID")"
assert_contains "$local_create" "Created fast VM at $STORE/vms/$VM_LOCAL.vmbridge" "local create"
assert_contains "$local_create" "Native optimized path available on Apple Silicon." "local create"
assert_template_manifest \
  "local create" \
  "$VM_LOCAL" \
  "ubuntu" \
  "linux-installer" \
  "installerImage" \
  "installers/ubuntu-arm64.iso"

assert_template_create \
  "local fedora template" \
  "template-local-fedora" \
  "fedora-arm64-installer" \
  "fedora" \
  "linux-installer" \
  "installerImage" \
  "installers/fedora-arm64.iso" \
  bridgevm

assert_template_create \
  "local debian template" \
  "template-local-debian" \
  "debian-arm64-installer" \
  "debian" \
  "linux-installer" \
  "installerImage" \
  "installers/debian-arm64.iso" \
  bridgevm

assert_template_create \
  "local macos template" \
  "template-local-macos" \
  "macos-restore" \
  "macos" \
  "macos-restore" \
  "macosRestoreImage" \
  "installers/macos-restore.ipsw" \
  bridgevm

local_list="$(bridgevm list)"
assert_contains "$local_list" "$VM_LOCAL" "local list"
assert_contains "$local_list" "template-local-fedora" "local list"
assert_contains "$local_list" "template-local-debian" "local list"
assert_contains "$local_list" "template-local-macos" "local list"
assert_contains "$local_list" "stopped" "local list"
assert_contains "$local_list" "fast" "local list"
assert_contains "$local_list" "ubuntu arm64" "local list"
assert_contains "$local_list" "fedora arm64" "local list"
assert_contains "$local_list" "debian arm64" "local list"
assert_contains "$local_list" "macos arm64" "local list"

assert_fails_contains \
  "unknown-template-rejected" \
  "unknown template id: bridgevm-missing-template" \
  bridgevm create "template-missing" --template bridgevm-missing-template
[[ ! -e "$STORE/vms/template-missing.vmbridge" ]] \
  || fail "unknown template rejection created a VM bundle"

assert_fails_contains \
  "duplicate-local-create-rejected" \
  "already exists" \
  bridgevm create "$VM_LOCAL" --template "$TEMPLATE_ID"

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

socket_templates="$(bridgevm_socket templates)"
assert_template_listing "socket templates" "$socket_templates"

socket_create="$(bridgevm_socket create "$VM_SOCKET" --template "$TEMPLATE_ID")"
assert_contains "$socket_create" "$VM_SOCKET" "socket create"
assert_contains "$socket_create" "stopped" "socket create"
assert_contains "$socket_create" "fast" "socket create"
assert_contains "$socket_create" "ubuntu arm64" "socket create"
assert_template_manifest \
  "socket create" \
  "$VM_SOCKET" \
  "ubuntu" \
  "linux-installer" \
  "installerImage" \
  "installers/ubuntu-arm64.iso"

assert_template_create \
  "socket fedora template" \
  "template-socket-fedora" \
  "fedora-arm64-installer" \
  "fedora" \
  "linux-installer" \
  "installerImage" \
  "installers/fedora-arm64.iso" \
  bridgevm_socket

assert_template_create \
  "socket debian template" \
  "template-socket-debian" \
  "debian-arm64-installer" \
  "debian" \
  "linux-installer" \
  "installerImage" \
  "installers/debian-arm64.iso" \
  bridgevm_socket

assert_template_create \
  "socket macos template" \
  "template-socket-macos" \
  "macos-restore" \
  "macos" \
  "macos-restore" \
  "macosRestoreImage" \
  "installers/macos-restore.ipsw" \
  bridgevm_socket

socket_list="$(bridgevm_socket list)"
assert_contains "$socket_list" "$VM_LOCAL" "socket list"
assert_contains "$socket_list" "$VM_SOCKET" "socket list"
assert_contains "$socket_list" "ubuntu arm64" "socket list"
assert_contains "$socket_list" "fedora arm64" "socket list"
assert_contains "$socket_list" "debian arm64" "socket list"
assert_contains "$socket_list" "macos arm64" "socket list"

assert_fails_contains \
  "duplicate-socket-create-rejected" \
  "already exists" \
  bridgevm_socket create "$VM_SOCKET" --template "$TEMPLATE_ID"

echo "PASS: template list/create CLI/socket integration smoke ($STORE)"
