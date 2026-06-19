#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-template-create.XXXXXX")"
VM_LOCAL="template-local"
VM_SOCKET="template-socket"
TEMPLATE_ID="ubuntu-arm64-installer"
VZ_TEMPLATE_ID="debian-arm64-apple-vz-linux-kernel-raw"
UBUNTU_VZ_TEMPLATE_ID="ubuntu-arm64-apple-vz-linux-kernel-raw"
VM_LOCAL_VZ="template-local-vz-linux"
VM_SOCKET_VZ="template-socket-vz-linux"
VM_LOCAL_UBUNTU_VZ="template-local-ubuntu-vz-linux"
VM_SOCKET_UBUNTU_VZ="template-socket-ubuntu-vz-linux"
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
  assert_contains "$output" "Boot template id: $UBUNTU_VZ_TEMPLATE_ID" "$label"
  assert_contains "$output" "Boot media: Ubuntu arm64 Apple VZ linux-kernel raw-disk desktop path" "$label"
  assert_contains "$output" "Kernel command line: console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target" "$label"
  assert_contains "$output" "Primary disk size: 32GiB" "$label"
  assert_contains "$output" "Boot template id: fedora-arm64-installer" "$label"
  assert_contains "$output" "Installer image: installers/fedora-arm64.iso" "$label"
  assert_contains "$output" "Boot template id: debian-arm64-installer" "$label"
  assert_contains "$output" "Installer image: installers/debian-arm64.iso" "$label"
  assert_contains "$output" "Boot template id: $VZ_TEMPLATE_ID" "$label"
  assert_contains "$output" "Guest: debian arm64" "$label"
  assert_contains "$output" "Boot template: linux-kernel" "$label"
  assert_contains "$output" "Boot media: Debian arm64 Apple VZ linux-kernel raw-disk demo" "$label"
  assert_contains "$output" "Kernel path: boot/vmlinuz" "$label"
  assert_contains "$output" "Initrd path: boot/initrd" "$label"
  assert_contains "$output" "Kernel command line: console=hvc0 priority=low" "$label"
  assert_contains "$output" "Primary disk path: disks/root.raw" "$label"
  assert_contains "$output" "Primary disk format: raw" "$label"
  assert_contains "$output" "Primary disk size: 64MiB" "$label"
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

assert_vz_template_manifest() {
  local label="$1"
  local vm="$2"
  local manifest="$STORE/vms/$vm.vmbridge/manifest.yaml"

  [[ -f "$manifest" ]] || fail "$label manifest missing: $manifest"
  grep -q "name: $vm" "$manifest" || fail "$label manifest omitted name"
  grep -q "mode: fast" "$manifest" || fail "$label manifest omitted fast mode"
  grep -q "os: debian" "$manifest" || fail "$label manifest omitted guest os"
  grep -q "arch: arm64" "$manifest" || fail "$label manifest omitted guest arch"
  grep -q "path: disks/root.raw" "$manifest" || fail "$label manifest omitted raw disk path"
  grep -q "size: 64MiB" "$manifest" || fail "$label manifest omitted raw disk size"
  grep -q "format: raw" "$manifest" || fail "$label manifest omitted raw disk format"
  grep -q "mode: linux-kernel" "$manifest" || fail "$label manifest omitted linux-kernel boot"
  grep -q "kernelPath: boot/vmlinuz" "$manifest" || fail "$label manifest omitted kernel path"
  grep -q "initrdPath: boot/initrd" "$manifest" || fail "$label manifest omitted initrd path"
  grep -q "kernelCommandLine: console=hvc0 priority=low" "$manifest" \
    || fail "$label manifest omitted kernel command line"
}

assert_ubuntu_vz_template_manifest() {
  local label="$1"
  local vm="$2"
  local manifest="$STORE/vms/$vm.vmbridge/manifest.yaml"

  [[ -f "$manifest" ]] || fail "$label manifest missing: $manifest"
  grep -q "name: $vm" "$manifest" || fail "$label manifest omitted name"
  grep -q "mode: fast" "$manifest" || fail "$label manifest omitted fast mode"
  grep -q "os: ubuntu" "$manifest" || fail "$label manifest omitted guest os"
  grep -q "arch: arm64" "$manifest" || fail "$label manifest omitted guest arch"
  grep -q "path: disks/root.raw" "$manifest" || fail "$label manifest omitted raw disk path"
  grep -q "size: 32GiB" "$manifest" || fail "$label manifest omitted raw disk size"
  grep -q "format: raw" "$manifest" || fail "$label manifest omitted raw disk format"
  grep -q "mode: linux-kernel" "$manifest" || fail "$label manifest omitted linux-kernel boot"
  grep -q "kernelPath: boot/vmlinuz" "$manifest" || fail "$label manifest omitted kernel path"
  grep -q "initrdPath: boot/initrd" "$manifest" || fail "$label manifest omitted initrd path"
  grep -q "kernelCommandLine: console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target" "$manifest" \
    || fail "$label manifest omitted Ubuntu graphical kernel command line"
}

stage_vz_template_files() {
  local vm="$1"
  local bundle="$STORE/vms/$vm.vmbridge"

  mkdir -p "$bundle/boot" "$bundle/disks"
  printf 'fake arm64 linux kernel fixture\n' >"$bundle/boot/vmlinuz"
  printf 'fake initrd fixture\n' >"$bundle/boot/initrd"
  truncate -s 1M "$bundle/disks/root.raw"
}

assert_vz_template_launch_ready_after_staging() {
  local label="$1"
  local vm="$2"
  local launch_spec="$STORE/vms/$vm.vmbridge/metadata/apple-vz-launch.json"
  local prepare

  stage_vz_template_files "$vm"
  prepare="$(bridgevm prepare-run "$vm")"
  assert_contains "$prepare" "Launch ready: true" "$label prepare-run"
  assert_contains "$prepare" "Disk format: raw" "$label prepare-run"
  assert_contains "$prepare" "Command: lightvm-runner --launch-spec $launch_spec" "$label prepare-run"
  grep -Fq '"mode": "linux-kernel"' "$launch_spec" \
    || fail "$label launch spec omitted linux-kernel boot mode"
  grep -Fq '"format": "raw"' "$launch_spec" \
    || fail "$label launch spec omitted raw disk format"
  grep -Fq '"ready": true' "$launch_spec" \
    || fail "$label launch spec was not ready"
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

local_vz_create="$(bridgevm create "$VM_LOCAL_VZ" --template "$VZ_TEMPLATE_ID")"
assert_contains "$local_vz_create" "Created fast VM at $STORE/vms/$VM_LOCAL_VZ.vmbridge" "local VZ template create"
assert_contains "$local_vz_create" "Native optimized path available on Apple Silicon." "local VZ template create"
assert_vz_template_manifest "local VZ template create" "$VM_LOCAL_VZ"
assert_vz_template_launch_ready_after_staging "local VZ template create" "$VM_LOCAL_VZ"

local_ubuntu_vz_create="$(bridgevm create "$VM_LOCAL_UBUNTU_VZ" --template "$UBUNTU_VZ_TEMPLATE_ID")"
assert_contains "$local_ubuntu_vz_create" "Created fast VM at $STORE/vms/$VM_LOCAL_UBUNTU_VZ.vmbridge" "local Ubuntu VZ template create"
assert_contains "$local_ubuntu_vz_create" "Native optimized path available on Apple Silicon." "local Ubuntu VZ template create"
assert_ubuntu_vz_template_manifest "local Ubuntu VZ template create" "$VM_LOCAL_UBUNTU_VZ"
assert_vz_template_launch_ready_after_staging "local Ubuntu VZ template create" "$VM_LOCAL_UBUNTU_VZ"

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
assert_contains "$local_list" "$VM_LOCAL_VZ" "local list"
assert_contains "$local_list" "$VM_LOCAL_UBUNTU_VZ" "local list"
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

socket_vz_create="$(bridgevm_socket create "$VM_SOCKET_VZ" --template "$VZ_TEMPLATE_ID")"
assert_contains "$socket_vz_create" "$VM_SOCKET_VZ" "socket VZ template create"
assert_contains "$socket_vz_create" "stopped" "socket VZ template create"
assert_contains "$socket_vz_create" "fast" "socket VZ template create"
assert_contains "$socket_vz_create" "debian arm64" "socket VZ template create"
assert_vz_template_manifest "socket VZ template create" "$VM_SOCKET_VZ"

socket_ubuntu_vz_create="$(bridgevm_socket create "$VM_SOCKET_UBUNTU_VZ" --template "$UBUNTU_VZ_TEMPLATE_ID")"
assert_contains "$socket_ubuntu_vz_create" "$VM_SOCKET_UBUNTU_VZ" "socket Ubuntu VZ template create"
assert_contains "$socket_ubuntu_vz_create" "stopped" "socket Ubuntu VZ template create"
assert_contains "$socket_ubuntu_vz_create" "fast" "socket Ubuntu VZ template create"
assert_contains "$socket_ubuntu_vz_create" "ubuntu arm64" "socket Ubuntu VZ template create"
assert_ubuntu_vz_template_manifest "socket Ubuntu VZ template create" "$VM_SOCKET_UBUNTU_VZ"

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
assert_contains "$socket_list" "$VM_LOCAL_VZ" "socket list"
assert_contains "$socket_list" "$VM_SOCKET_VZ" "socket list"
assert_contains "$socket_list" "$VM_LOCAL_UBUNTU_VZ" "socket list"
assert_contains "$socket_list" "$VM_SOCKET_UBUNTU_VZ" "socket list"
assert_contains "$socket_list" "ubuntu arm64" "socket list"
assert_contains "$socket_list" "fedora arm64" "socket list"
assert_contains "$socket_list" "debian arm64" "socket list"
assert_contains "$socket_list" "macos arm64" "socket list"

assert_fails_contains \
  "duplicate-socket-create-rejected" \
  "already exists" \
  bridgevm_socket create "$VM_SOCKET" --template "$TEMPLATE_ID"

echo "PASS: template list/create CLI/socket integration smoke ($STORE)"
