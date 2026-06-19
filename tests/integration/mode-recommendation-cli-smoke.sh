#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- "$@"
}

fail() {
  echo "FAIL: $*" >&2
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

ubuntu_output="$(bridgevm recommend --os ubuntu --arch arm64)"
assert_contains "$ubuntu_output" "Recommended mode: fast" "Ubuntu arm64 recommendation"
assert_contains "$ubuntu_output" "Current execution engine: Apple VZ Engine (apple-vz)" "Ubuntu arm64 recommendation"
assert_contains "$ubuntu_output" "Current engine QEMU usage: not used" "Ubuntu arm64 recommendation"
assert_contains "$ubuntu_output" "Boot template id: ubuntu-arm64-installer" "Ubuntu arm64 recommendation"
assert_contains "$ubuntu_output" "Installer image: installers/ubuntu-arm64.iso" "Ubuntu arm64 recommendation"

fedora_output="$(bridgevm recommend --os fedora --arch arm64)"
assert_contains "$fedora_output" "Recommended mode: fast" "Fedora arm64 recommendation"
assert_contains "$fedora_output" "Boot template id: fedora-arm64-installer" "Fedora arm64 recommendation"
assert_contains "$fedora_output" "Installer image: installers/fedora-arm64.iso" "Fedora arm64 recommendation"

debian_output="$(bridgevm recommend --os debian --arch arm64)"
assert_contains "$debian_output" "Recommended mode: fast" "Debian arm64 recommendation"
assert_contains "$debian_output" "Boot template id: debian-arm64-installer" "Debian arm64 recommendation"
assert_contains "$debian_output" "Installer image: installers/debian-arm64.iso" "Debian arm64 recommendation"

macos_output="$(bridgevm recommend --os macos --arch arm64)"
assert_contains "$macos_output" "Recommended mode: fast" "macOS arm64 recommendation"
assert_contains "$macos_output" "Boot template id: macos-restore" "macOS arm64 recommendation"
assert_contains "$macos_output" "Boot template: macos-restore" "macOS arm64 recommendation"
assert_contains "$macos_output" "macOS restore image: installers/macos-restore.ipsw" "macOS arm64 recommendation"

windows_output="$(bridgevm recommend --os windows --version 11 --arch arm64)"
assert_contains "$windows_output" "Recommended mode: compatibility" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Current execution engine: QEMU Compatibility Engine (qemu-compatibility)" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Current engine QEMU usage: required" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Target product engine: BridgeVM HVF Engine (bridge-hvf)" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Target engine substrate: Apple Hypervisor.framework plus BridgeVM VMM/device stack" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Target engine QEMU usage: not used" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Windows is not bootable yet" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "restricted QEMU/HVF backend today" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "Apple VZ Fast Mode is Linux/macOS Arm only" "Windows 11 Arm recommendation"
assert_contains "$windows_output" "must not claim Microsoft-authorized or Parallels-class Windows support" "Windows 11 Arm recommendation"
assert_not_contains "$windows_output" "Boot template id:" "Windows 11 Arm recommendation"

x86_output="$(bridgevm recommend --os ubuntu --arch x86_64)"
assert_contains "$x86_output" "Recommended mode: compatibility" "Ubuntu x86_64 recommendation"
assert_contains "$x86_output" "Use Compatibility Mode instead" "Ubuntu x86_64 recommendation"

echo "PASS: mode recommendation CLI smoke"
