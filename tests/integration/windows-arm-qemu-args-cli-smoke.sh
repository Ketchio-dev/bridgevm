#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-qemu.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="win11-arm"
VNC_VM_NAME="win11-arm-vnc"
INSTALLER_VM_NAME="win11-arm-installer"
VNC_MANIFEST="$STORE/vms/$VNC_VM_NAME.vmbridge/manifest.yaml"
INSTALLER_ISO="/tmp/bridgevm-fake-win11-arm.iso"
BACKEND_LOG="$STORE/backend-launch.log"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

for backend in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend launch is forbidden in Windows Arm qemu-args planning smoke: $(basename "$0")" >&2
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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
  esac
}

assert_restricted_windows_arm_qemu_args() {
  local output="$1"
  local label="$2"

  assert_contains "$output" "qemu-system-aarch64" "$label"
  assert_contains "$output" "-machine" "$label"
  assert_contains "$output" "virt" "$label"
  assert_contains "$output" "-accel" "$label"
  assert_contains "$output" "hvf" "$label"
  assert_contains "$output" "-display" "$label"
  assert_contains "$output" "cocoa,gl=on" "$label"
  assert_contains "$output" "-device" "$label"
  assert_contains "$output" "virtio-rng-pci" "$label"
  assert_contains "$output" "-cpu" "$label"
  assert_contains "$output" "host" "$label"
  assert_contains "$output" "-bios" "$label"
  assert_contains "$output" "edk2-aarch64-code.fd" "$label"
  assert_contains "$output" "socket,id=bridgevm-tools" "$label"
  assert_contains "$output" "virtserialport,chardev=bridgevm-tools,name=org.bridgevm.guest-tools.0" "$label"
  assert_not_contains "$output" "qemu-system-x86_64" "$label"
  assert_not_contains "$output" "AppleVzRunner" "$label"
  assert_not_contains "$output" "default,show-cursor=on" "$label"
}

assert_external_vnc_viewer_handoff() {
  local output="$1"
  local label="$2"

  assert_contains "$output" "qemu-system-aarch64" "$label"
  assert_contains "$output" "-machine" "$label"
  assert_contains "$output" "virt" "$label"
  assert_contains "$output" "-accel" "$label"
  assert_contains "$output" "hvf" "$label"
  assert_contains "$output" "-bios" "$label"
  assert_contains "$output" "edk2-aarch64-code.fd" "$label"
  assert_contains "$output" "-display" "$label"
  assert_contains "$output" "vnc=:0" "$label"
  assert_contains "$output" "virtio-rng-pci" "$label"
  assert_not_contains "$output" "cocoa,gl=on" "$label"
  assert_not_contains "$output" "spice" "$label"
  assert_not_contains "$output" "AppleVzRunner" "$label"
}

assert_windows_installer_qemu_args() {
  local output="$1"
  local label="$2"

  # Restricted Windows Arm base shape is still present.
  assert_contains "$output" "qemu-system-aarch64" "$label"
  assert_contains "$output" "edk2-aarch64-code.fd" "$label"
  assert_contains "$output" "virtio-rng-pci" "$label"
  # Installer-media device shape.
  assert_contains "$output" "ramfb" "$label"
  assert_contains "$output" "qemu-xhci,id=usb" "$label"
  assert_contains "$output" "usb-kbd,bus=usb.0" "$label"
  assert_contains "$output" "usb-storage,bus=usb.0,drive=installer,bootindex=0" "$label"
  assert_contains "$output" "if=none,id=installer,file=$INSTALLER_ISO,media=cdrom,readonly=on" "$label"
  assert_not_contains "$output" "qemu-system-x86_64" "$label"
  assert_not_contains "$output" "AppleVzRunner" "$label"
}

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend launch attempted: $(cat "$BACKEND_LOG")"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os windows --version 11 --arch arm64 --mode compatibility >/dev/null
bridgevm create "$VNC_VM_NAME" --os windows --version 11 --arch arm64 --mode compatibility >/dev/null
perl -0pi -e 's/display:\n  renderer: spice-or-vnc/display:\n  renderer: vnc/' "$VNC_MANIFEST"
bridgevm create "$INSTALLER_VM_NAME" --os windows --version 11 --arch arm64 \
  --mode compatibility --boot-mode windows-installer --installer-image "$INSTALLER_ISO" >/dev/null

grep -q "version: '11'" "$STORE/vms/$VM_NAME.vmbridge/manifest.yaml" \
  || fail "manifest did not record Windows 11 version"
grep -q "arch: arm64" "$STORE/vms/$VM_NAME.vmbridge/manifest.yaml" \
  || fail "manifest did not record arm64 arch"

local_args="$(bridgevm qemu-args "$VM_NAME")"
assert_restricted_windows_arm_qemu_args "$local_args" "local Windows 11 Arm qemu-args"
assert_no_backend_launch

local_vnc_args="$(bridgevm qemu-args "$VNC_VM_NAME")"
assert_external_vnc_viewer_handoff "$local_vnc_args" "local Windows 11 Arm external VNC viewer handoff"
assert_no_backend_launch

local_installer_args="$(bridgevm qemu-args "$INSTALLER_VM_NAME")"
assert_windows_installer_qemu_args "$local_installer_args" "local Windows 11 Arm installer qemu-args"
assert_no_backend_launch

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..600}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

socket_args="$(bridgevm_socket qemu-args "$VM_NAME")"
assert_restricted_windows_arm_qemu_args "$socket_args" "socket Windows 11 Arm qemu-args"
assert_no_backend_launch

socket_vnc_args="$(bridgevm_socket qemu-args "$VNC_VM_NAME")"
assert_external_vnc_viewer_handoff "$socket_vnc_args" "socket Windows 11 Arm external VNC viewer handoff"
assert_no_backend_launch

socket_installer_args="$(bridgevm_socket qemu-args "$INSTALLER_VM_NAME")"
assert_windows_installer_qemu_args "$socket_installer_args" "socket Windows 11 Arm installer qemu-args"
assert_no_backend_launch

echo "PASS: Windows 11 Arm restricted QEMU args CLI/socket smoke ($STORE)"
