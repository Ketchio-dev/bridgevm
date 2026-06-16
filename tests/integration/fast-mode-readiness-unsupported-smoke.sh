#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-ready2.XXXXXX")"
VM_NAME="fast-unsupported"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
MANIFEST="$BUNDLE/manifest.yaml"
BASE_MANIFEST="$STORE/base-manifest.yaml"
DISK="$BUNDLE/disks/root.qcow2"
INSTALLER="$BUNDLE/media/ubuntu.iso"
KERNEL="$BUNDLE/boot/vmlinuz"
INITRD="$BUNDLE/boot/initrd"
RESTORE_IMAGE="$BUNDLE/installers/macos-restore.ipsw"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

lightvm_runner() {
  cargo run --quiet -p lightvm-runner -- --store "$STORE" "$@"
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
  assert_not_contains "$output" "Launch ready:" "$label"
}

assert_command_contains() {
  local label="$1"
  shift

  local output
  output="$("$@")"
  assert_contains "$output" "Launch ready: false" "$label"
  ASSERT_OUTPUT="$output"
}

assert_command_succeeds() {
  local label="$1"
  shift

  local output
  output="$("$@")"
  [[ -n "$output" ]] || fail "$label produced no output"
  ASSERT_OUTPUT="$output"
}

assert_no_runner_metadata() {
  local label="$1"
  shift

  local output
  output="$("$@")"
  assert_contains "$output" "No runner metadata" "$label"
}

reset_manifest() {
  cp "$BASE_MANIFEST" "$MANIFEST"
  clear_disk_metadata
}

clear_runner_metadata() {
  rm -f "$BUNDLE/metadata/runner.json"
}

clear_disk_metadata() {
  rm -f "$BUNDLE/metadata/active-disk.json" "$BUNDLE/metadata/primary-disk.json"
}

set_boot_section() {
  local replacement="$1"
  perl -0pi -e "s#boot:\n  mode: linux-installer\n  installerImage: media/ubuntu\\.iso#$replacement#" "$MANIFEST"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" \
  --os ubuntu \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-installer \
  --installer-image media/ubuntu.iso >/dev/null

cp "$MANIFEST" "$BASE_MANIFEST"

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

reset_manifest
mkdir -p "$(dirname "$INSTALLER")"
: >"$INSTALLER"
rm -f "$DISK"
assert_command_contains "local-missing-disk-prepare" bridgevm prepare-run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-primary-disk" "local missing disk prepare"
assert_contains "$ASSERT_OUTPUT" "$DISK" "local missing disk prepare"
assert_not_contains "$ASSERT_OUTPUT" "missing-installer-image" "local missing disk prepare"
assert_command_contains "socket-missing-disk-run" bridgevm_socket run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-primary-disk" "socket missing disk run"
assert_contains "$ASSERT_OUTPUT" "$DISK" "socket missing disk run"
assert_not_contains "$ASSERT_OUTPUT" "missing-installer-image" "socket missing disk run"

reset_manifest
clear_runner_metadata
clear_disk_metadata
perl -0pi -e 's/format: qcow2/format: vmdk/' "$MANIFEST"
assert_fails_contains \
  "local-disk-format-prepare" \
  "Apple VZ launch requires primary disk format raw/qcow2, got vmdk" \
  bridgevm prepare-run "$VM_NAME"
assert_fails_contains \
  "local-disk-format-run" \
  "Apple VZ launch requires primary disk format raw/qcow2, got vmdk" \
  bridgevm run "$VM_NAME"
assert_fails_contains \
  "socket-disk-format-prepare" \
  "Apple VZ launch requires primary disk format raw/qcow2, got vmdk" \
  bridgevm_socket prepare-run "$VM_NAME"
assert_fails_contains \
  "socket-disk-format-run" \
  "Apple VZ launch requires primary disk format raw/qcow2, got vmdk" \
  bridgevm_socket run "$VM_NAME"
assert_no_runner_metadata "local-disk-format-runner-status" bridgevm runner-status "$VM_NAME"
assert_no_runner_metadata "socket-disk-format-runner-status" bridgevm_socket runner-status "$VM_NAME"

reset_manifest
mkdir -p "$(dirname "$DISK")"
: >"$DISK"
rm -f "$KERNEL" "$INITRD"
set_boot_section $'boot:\n  mode: linux-kernel\n  kernelPath: boot/vmlinuz\n  initrdPath: boot/initrd'
assert_command_contains "local-linux-kernel-missing-media-prepare" bridgevm prepare-run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-kernel" "local linux-kernel missing media prepare"
assert_contains "$ASSERT_OUTPUT" "$KERNEL" "local linux-kernel missing media prepare"
assert_contains "$ASSERT_OUTPUT" "missing-initrd" "local linux-kernel missing media prepare"
assert_contains "$ASSERT_OUTPUT" "$INITRD" "local linux-kernel missing media prepare"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "local linux-kernel missing media prepare"
assert_not_contains "$ASSERT_OUTPUT" "missing-installer-image" "local linux-kernel missing media prepare"
mkdir -p "$(dirname "$KERNEL")"
: >"$KERNEL"
assert_command_contains "socket-linux-kernel-missing-initrd-run" bridgevm_socket run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-initrd" "socket linux-kernel missing initrd run"
assert_contains "$ASSERT_OUTPUT" "$INITRD" "socket linux-kernel missing initrd run"
assert_not_contains "$ASSERT_OUTPUT" "missing-kernel" "socket linux-kernel missing initrd run"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "socket linux-kernel missing initrd run"
assert_not_contains "$ASSERT_OUTPUT" "missing-installer-image" "socket linux-kernel missing initrd run"

reset_manifest
mkdir -p "$(dirname "$DISK")"
: >"$DISK"
rm -f "$RESTORE_IMAGE"
perl -0pi -e 's/os: ubuntu/os: macos/' "$MANIFEST"
set_boot_section $'boot:\n  mode: macos-restore\n  macosRestoreImage: installers/macos-restore.ipsw'
assert_command_contains "local-macos-restore-missing-image-prepare" bridgevm prepare-run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-macos-restore-image" "local macos restore missing image prepare"
assert_contains "$ASSERT_OUTPUT" "$RESTORE_IMAGE" "local macos restore missing image prepare"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "local macos restore missing image prepare"
assert_command_contains "socket-macos-restore-missing-image-run" bridgevm_socket run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "missing-macos-restore-image" "socket macos restore missing image run"
assert_contains "$ASSERT_OUTPUT" "$RESTORE_IMAGE" "socket macos restore missing image run"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "socket macos restore missing image run"

reset_manifest
clear_runner_metadata
perl -0pi -e 's/arch: arm64/arch: x86_64/' "$MANIFEST"
assert_fails_contains \
  "local-x86-prepare" \
  "Apple VZ launch requires guest arch arm64/aarch64, got x86_64" \
  bridgevm prepare-run "$VM_NAME"
assert_fails_contains \
  "local-x86-run" \
  "Apple VZ launch requires guest arch arm64/aarch64, got x86_64" \
  bridgevm run "$VM_NAME"
assert_fails_contains \
  "socket-x86-prepare" \
  "Apple VZ launch requires guest arch arm64/aarch64, got x86_64" \
  bridgevm_socket prepare-run "$VM_NAME"
assert_fails_contains \
  "socket-x86-run" \
  "Apple VZ launch requires guest arch arm64/aarch64, got x86_64" \
  bridgevm_socket run "$VM_NAME"
assert_no_runner_metadata "local-x86-runner-status" bridgevm runner-status "$VM_NAME"
assert_no_runner_metadata "socket-x86-runner-status" bridgevm_socket runner-status "$VM_NAME"

reset_manifest
clear_runner_metadata
perl -0pi -e 's/preferred: apple-vz/preferred: qemu/' "$MANIFEST"
assert_fails_contains \
  "local-backend-prepare" \
  "Apple VZ launch requires backend preferred apple-vz or unset, got qemu" \
  bridgevm prepare-run "$VM_NAME"
assert_fails_contains \
  "local-backend-run" \
  "Apple VZ launch requires backend preferred apple-vz or unset, got qemu" \
  bridgevm run "$VM_NAME"
assert_fails_contains \
  "socket-backend-prepare" \
  "Apple VZ launch requires backend preferred apple-vz or unset, got qemu" \
  bridgevm_socket prepare-run "$VM_NAME"
assert_fails_contains \
  "socket-backend-run" \
  "Apple VZ launch requires backend preferred apple-vz or unset, got qemu" \
  bridgevm_socket run "$VM_NAME"
assert_no_runner_metadata "local-backend-runner-status" bridgevm runner-status "$VM_NAME"
assert_no_runner_metadata "socket-backend-runner-status" bridgevm_socket runner-status "$VM_NAME"

reset_manifest
clear_runner_metadata
perl -0pi -e 's/network:\n  mode: nat/network:\n  mode: bridged/' "$MANIFEST"
assert_fails_contains \
  "local-network-prepare" \
  "Apple VZ launch requires nat networking, got bridged" \
  bridgevm prepare-run "$VM_NAME"
assert_fails_contains \
  "local-network-run" \
  "Apple VZ launch requires nat networking, got bridged" \
  bridgevm run "$VM_NAME"
assert_fails_contains \
  "socket-network-prepare" \
  "Apple VZ launch requires nat networking, got bridged" \
  bridgevm_socket prepare-run "$VM_NAME"
assert_fails_contains \
  "socket-network-run" \
  "Apple VZ launch requires nat networking, got bridged" \
  bridgevm_socket run "$VM_NAME"
assert_no_runner_metadata "local-network-runner-status" bridgevm runner-status "$VM_NAME"
assert_no_runner_metadata "socket-network-runner-status" bridgevm_socket runner-status "$VM_NAME"

reset_manifest
mkdir -p "$(dirname "$DISK")" "$(dirname "$KERNEL")"
: >"$DISK"
: >"$KERNEL"
set_boot_section $'boot:\n  mode: linux-kernel\n  kernelPath: boot/vmlinuz'
assert_command_succeeds "local-qcow2-linux-kernel-ready-run" bridgevm run "$VM_NAME"
assert_contains "$ASSERT_OUTPUT" "Launch ready: true" "local qcow2 linux-kernel ready run"
assert_not_contains "$ASSERT_OUTPUT" "missing-kernel" "local qcow2 linux-kernel ready run"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "local qcow2 linux-kernel ready run"
handoff_qcow2_output="$(lightvm_runner --launch-spec "$LAUNCH_SPEC" --require-ready --print-handoff)"
assert_contains "$handoff_qcow2_output" '"boot_mode": "linux-kernel"' "qcow2 linux-kernel handoff"
assert_contains "$handoff_qcow2_output" "\"path\": \"$DISK\"" "qcow2 linux-kernel handoff"
assert_contains "$handoff_qcow2_output" '"format": "qcow2"' "qcow2 linux-kernel handoff"
assert_contains "$handoff_qcow2_output" '"ready": true' "qcow2 linux-kernel handoff"
QCOW2_HANDOFF_JSON="$STORE/qcow2-linux-kernel-handoff.json"
printf "%s\n" "$handoff_qcow2_output" >"$QCOW2_HANDOFF_JSON"
assert_fails_contains \
  "apple-vz-runner-qcow2-vz-config-validate" \
  "AppleVzRunner requires disk format raw/qcow2, got qcow2" \
  bash -c "cd apps/macos && swift run --quiet AppleVzRunner --handoff-json '$QCOW2_HANDOFF_JSON' --validate-only --print-config-plan --validate-vz-config"
assert_contains "$ASSERT_OUTPUT" "AppleVzRunner handoff ready" "AppleVzRunner qcow2 config validation"
assert_contains "$ASSERT_OUTPUT" "Configuration plan:" "AppleVzRunner qcow2 config validation"
assert_contains "$ASSERT_OUTPUT" "Disk attachment: disk-image-qcow2" "AppleVzRunner qcow2 config validation"
assert_not_contains "$ASSERT_OUTPUT" "VZ configuration validation: ready" "AppleVzRunner qcow2 config validation"

echo "PASS: Fast Mode unsupported readiness CLI/socket smoke ($STORE)"
