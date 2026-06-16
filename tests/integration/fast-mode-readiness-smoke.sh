#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-ready.XXXXXX")"
VM_NAME="fast-linux"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
DISK="$BUNDLE/disks/root.qcow2"
INSTALLER="$BUNDLE/media/ubuntu.iso"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
RUNNER_METADATA="$BUNDLE/metadata/runner.json"

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
}

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  if ! grep -Fq "$needle" "$file"; then
    fail "$label missing '$needle' in $file"
  fi
}

assert_runner_launch_spec_path() {
  local label="$1"
  [[ -f "$RUNNER_METADATA" ]] || fail "$label missing file $RUNNER_METADATA"
  if ! EXPECTED_LAUNCH_SPEC="$LAUNCH_SPEC" perl -0ne 'exit(/"launch_spec_path"\s*:\s*"\Q$ENV{EXPECTED_LAUNCH_SPEC}\E"/ ? 0 : 1)' "$RUNNER_METADATA"; then
    fail "$label launch_spec_path did not match $LAUNCH_SPEC in $RUNNER_METADATA"
  fi
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

prepare_output="$(bridgevm prepare-run "$VM_NAME")"
assert_contains "$prepare_output" "Engine: lightvm" "prepare-run output"
assert_contains "$prepare_output" "Dry run: true" "prepare-run output"
assert_contains "$prepare_output" "Launch spec: $LAUNCH_SPEC" "prepare-run output"
assert_contains "$prepare_output" "Launch ready: false" "prepare-run output"
assert_contains "$prepare_output" "missing-primary-disk" "prepare-run output"
assert_contains "$prepare_output" "$DISK" "prepare-run output"
assert_contains "$prepare_output" "missing-installer-image" "prepare-run output"
assert_contains "$prepare_output" "$INSTALLER" "prepare-run output"
assert_file_contains "$LAUNCH_SPEC" '"vm_name": "fast-linux"' "prepare-run launch spec"
assert_file_contains "$LAUNCH_SPEC" '"ready": false' "prepare-run launch spec"
assert_runner_launch_spec_path "prepare-run runner metadata"
assert_fails_contains \
  "lightvm-runner-require-ready-blocked" \
  "Fast Mode launch readiness failed" \
  lightvm_runner "$VM_NAME" --require-ready
assert_file_contains "$LAUNCH_SPEC" '"missing-primary-disk"' "blocked lightvm-runner launch spec"
assert_fails_contains \
  "lightvm-runner-launch-spec-require-ready-blocked" \
  "Fast Mode launch readiness failed" \
  lightvm_runner --launch-spec "$LAUNCH_SPEC" --require-ready
handoff_blocked_output="$(lightvm_runner --launch-spec "$LAUNCH_SPEC" --print-handoff)"
assert_contains "$handoff_blocked_output" '"backend": "apple-virtualization-framework"' "blocked handoff output"
assert_contains "$handoff_blocked_output" '"vm_name": "fast-linux"' "blocked handoff output"
assert_contains "$handoff_blocked_output" "\"bundle_path\": \"$BUNDLE\"" "blocked handoff output"
assert_contains "$handoff_blocked_output" "\"launch_spec_path\": \"$LAUNCH_SPEC\"" "blocked handoff output"
assert_contains "$handoff_blocked_output" '"os": "ubuntu"' "blocked handoff output"
assert_contains "$handoff_blocked_output" '"arch": "arm64"' "blocked handoff output"
assert_contains "$handoff_blocked_output" '"boot_mode": "linux-installer"' "blocked handoff output"
assert_contains "$handoff_blocked_output" "\"path\": \"$DISK\"" "blocked handoff output"
assert_contains "$handoff_blocked_output" '"format": "qcow2"' "blocked handoff output"
assert_contains "$handoff_blocked_output" '"missing-primary-disk"' "blocked handoff output"

runner_output="$(bridgevm runner-status "$VM_NAME")"
assert_contains "$runner_output" "Launch spec: $LAUNCH_SPEC" "local runner-status output"
assert_contains "$runner_output" "Launch ready: false" "local runner-status output"
assert_contains "$runner_output" "missing-primary-disk" "local runner-status output"
assert_contains "$runner_output" "missing-installer-image" "local runner-status output"

assert_fails_contains \
  "local-run-spawn-blocked" \
  "Fast Mode spawn is not implemented yet" \
  bridgevm run "$VM_NAME" --spawn
assert_contains "$ASSERT_OUTPUT" "launch blockers:" "local run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "missing-primary-disk" "local run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "$DISK" "local run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "missing-installer-image" "local run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "$INSTALLER" "local run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "fast-mode-spawn-unimplemented" "local run --spawn failure"

mkdir -p "$(dirname "$DISK")" "$(dirname "$INSTALLER")"
: >"$DISK"
: >"$INSTALLER"

run_output="$(bridgevm run "$VM_NAME")"
assert_contains "$run_output" "Engine: lightvm" "local run output"
assert_contains "$run_output" "Dry run: true" "local run output"
assert_contains "$run_output" "Launch spec: $LAUNCH_SPEC" "local run output"
assert_contains "$run_output" "Launch ready: true" "local run output"
assert_not_contains "$run_output" "missing-primary-disk" "local run output"
assert_not_contains "$run_output" "missing-installer-image" "local run output"
assert_file_contains "$LAUNCH_SPEC" '"ready": true' "local run launch spec"
assert_runner_launch_spec_path "local run runner metadata"
runner_ready_output="$(lightvm_runner "$VM_NAME" --require-ready --print-plan)"
assert_contains "$runner_ready_output" '"ready": true' "ready lightvm-runner output"
assert_file_contains "$LAUNCH_SPEC" '"ready": true' "ready lightvm-runner launch spec"
handoff_ready_output="$(lightvm_runner --launch-spec "$LAUNCH_SPEC" --require-ready --print-handoff)"
assert_contains "$handoff_ready_output" '"backend": "apple-virtualization-framework"' "ready handoff output"
assert_contains "$handoff_ready_output" '"ready": true' "ready handoff output"
assert_contains "$handoff_ready_output" '"vm_name": "fast-linux"' "ready handoff output"
assert_contains "$handoff_ready_output" "\"launch_spec_path\": \"$LAUNCH_SPEC\"" "ready handoff output"
assert_contains "$handoff_ready_output" '"boot_mode": "linux-installer"' "ready handoff output"
assert_contains "$handoff_ready_output" "\"path\": \"$DISK\"" "ready handoff output"
assert_contains "$handoff_ready_output" '"resources": {' "ready handoff output"
assert_contains "$handoff_ready_output" '"memory": "4096"' "ready handoff output"
assert_contains "$handoff_ready_output" '"cpu": "2"' "ready handoff output"
READY_HANDOFF_JSON="$STORE/ready-handoff.json"
printf "%s\n" "$handoff_ready_output" >"$READY_HANDOFF_JSON"
apple_vz_runner_validate_output="$(
  cd apps/macos
  swift run --quiet AppleVzRunner --handoff-json "$READY_HANDOFF_JSON" --validate-only --print-config-plan
)"
assert_contains "$apple_vz_runner_validate_output" "AppleVzRunner handoff ready" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "VM: fast-linux" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "Boot mode: linux-installer" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "Memory MiB: 4096" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "CPU count: 2" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "Configuration plan:" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "Boot loader: efi" "AppleVzRunner validate output"
assert_contains "$apple_vz_runner_validate_output" "Disk attachment: disk-image-qcow2" "AppleVzRunner validate output"
VZ_BUNDLE="$STORE/vms/vz-linux-kernel.vmbridge"
VZ_DISK="$VZ_BUNDLE/disks/root.raw"
VZ_KERNEL="$VZ_BUNDLE/boot/vmlinuz"
VZ_LAUNCH_SPEC="$VZ_BUNDLE/metadata/apple-vz-launch.json"
VZ_HANDOFF_JSON="$STORE/vz-linux-kernel-handoff.json"
VZ_RUNNER_LOG="$VZ_BUNDLE/logs/lightvm.log"
VZ_SERIAL_LOG="$VZ_BUNDLE/logs/serial.log"
mkdir -p "$VZ_BUNDLE/disks" "$VZ_BUNDLE/boot" "$VZ_BUNDLE/metadata" "$VZ_BUNDLE/logs"
truncate -s 64m "$VZ_DISK"
: >"$VZ_KERNEL"
cat >"$VZ_LAUNCH_SPEC" <<EOF
{
  "vm_name": "vz-linux-kernel",
  "bundle_path": "$VZ_BUNDLE",
  "guest": {
    "os": "ubuntu",
    "arch": "arm64"
  },
  "boot": {
    "mode": "linux-kernel",
    "kernel": {
      "path": "$VZ_KERNEL",
      "exists": true
    },
    "kernel_command_line": "console=hvc0 root=/dev/vda"
  },
  "disk": {
    "path": "$VZ_DISK",
    "format": "raw",
    "read_only": false
  },
  "resources": {
    "memory": "4096",
    "cpu": "2",
    "display_fps_cap": "60",
    "rationale": "Integration smoke VZ config validation fixture.",
    "balloon_device": true
  },
  "devices": {
    "entropy_device": true,
    "network": "nat",
    "serial_log_path": "$VZ_SERIAL_LOG"
  },
  "integration": {
    "clipboard": true,
    "dynamic_resolution": true,
    "shared_folders": true,
    "virtiofs": true
  },
  "logs": {
    "runner_log_path": "$VZ_RUNNER_LOG"
  },
  "readiness": {
    "ready": true,
    "blockers": []
  }
}
EOF
cat >"$VZ_HANDOFF_JSON" <<EOF
{
  "backend": "apple-virtualization-framework",
  "vm_name": "vz-linux-kernel",
  "bundle_path": "$VZ_BUNDLE",
  "launch_spec_path": "$VZ_LAUNCH_SPEC",
  "guest": {
    "os": "ubuntu",
    "arch": "arm64"
  },
  "boot_mode": "linux-kernel",
  "disk": {
    "path": "$VZ_DISK",
    "format": "raw",
    "read_only": false
  },
  "resources": {
    "memory": "4096",
    "cpu": "2",
    "display_fps_cap": "60",
    "rationale": "Integration smoke VZ config validation fixture.",
    "balloon_device": true
  },
  "runner_log_path": "$VZ_RUNNER_LOG",
  "serial_log_path": "$VZ_SERIAL_LOG",
  "integration": {
    "clipboard": true,
    "dynamic_resolution": true,
    "shared_folders": true,
    "virtiofs": true
  },
  "readiness": {
    "ready": true,
    "blockers": []
  }
}
EOF
VZ_CONFIG_STDOUT="$STORE/apple-vz-config-validate.stdout"
VZ_CONFIG_STDERR="$STORE/apple-vz-config-validate.stderr"
if (
  cd apps/macos
  swift run --quiet AppleVzRunner --handoff-json "$VZ_HANDOFF_JSON" --validate-only --print-config-plan --validate-vz-config
) >"$VZ_CONFIG_STDOUT" 2>"$VZ_CONFIG_STDERR"; then
  apple_vz_config_validate_output="$(cat "$VZ_CONFIG_STDOUT" "$VZ_CONFIG_STDERR")"
  assert_contains "$apple_vz_config_validate_output" "AppleVzRunner handoff ready" "AppleVzRunner VZ config validate output"
  assert_contains "$apple_vz_config_validate_output" "VM: vz-linux-kernel" "AppleVzRunner VZ config validate output"
  assert_contains "$apple_vz_config_validate_output" "Boot mode: linux-kernel" "AppleVzRunner VZ config validate output"
  assert_contains "$apple_vz_config_validate_output" "Disk attachment: disk-image-raw" "AppleVzRunner VZ config validate output"
  assert_contains "$apple_vz_config_validate_output" "Network attachment: nat" "AppleVzRunner VZ config validate output"
  assert_contains "$apple_vz_config_validate_output" "VZ configuration validation: ready" "AppleVzRunner VZ config validate output"
else
  apple_vz_config_validate_output="$(cat "$VZ_CONFIG_STDOUT" "$VZ_CONFIG_STDERR")"
  assert_contains "$apple_vz_config_validate_output" "Invalid virtual machine configuration." "AppleVzRunner VZ config entitlement output"
  assert_contains "$apple_vz_config_validate_output" "com.apple.security.virtualization" "AppleVzRunner VZ config entitlement output"
fi
APPLE_VZ_RUNNER_BIN="$(cd apps/macos && swift build --show-bin-path)/AppleVzRunner"
assert_fails_contains \
  "lightvm-runner-launch-spec-ready-launch-unimplemented" \
  "Apple Virtualization.framework launch is not implemented yet" \
  lightvm_runner --launch-spec "$LAUNCH_SPEC" --require-ready --launch
assert_fails_contains \
  "lightvm-runner-launch-spec-ready-swift-helper-unimplemented" \
  "real Apple VZ start requires --allow-real-vz-start" \
  lightvm_runner --launch-spec "$LAUNCH_SPEC" --require-ready --launch --apple-vz-runner "$APPLE_VZ_RUNNER_BIN"

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

socket_status_output="$(bridgevm_socket runner-status "$VM_NAME")"
assert_contains "$socket_status_output" "Engine: lightvm" "socket runner-status output"
assert_contains "$socket_status_output" "Dry run: true" "socket runner-status output"
assert_contains "$socket_status_output" "Launch spec: $LAUNCH_SPEC" "socket runner-status output"
assert_contains "$socket_status_output" "Launch ready: true" "socket runner-status output"

rm -f "$INSTALLER"

socket_run_output="$(bridgevm_socket run "$VM_NAME")"
assert_contains "$socket_run_output" "Engine: lightvm" "socket run output"
assert_contains "$socket_run_output" "Dry run: true" "socket run output"
assert_contains "$socket_run_output" "Launch spec: $LAUNCH_SPEC" "socket run output"
assert_contains "$socket_run_output" "Launch ready: false" "socket run output"
assert_contains "$socket_run_output" "missing-installer-image" "socket run output"
assert_contains "$socket_run_output" "$INSTALLER" "socket run output"
assert_not_contains "$socket_run_output" "missing-primary-disk" "socket run output"
assert_file_contains "$LAUNCH_SPEC" '"missing-installer-image"' "socket run launch spec"
assert_runner_launch_spec_path "socket run runner metadata"

assert_fails_contains \
  "socket-run-spawn-blocked" \
  "Fast Mode spawn is not implemented yet" \
  bridgevm_socket run "$VM_NAME" --spawn
assert_contains "$ASSERT_OUTPUT" "launch blockers:" "socket run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "missing-installer-image" "socket run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "$INSTALLER" "socket run --spawn failure"
assert_contains "$ASSERT_OUTPUT" "fast-mode-spawn-unimplemented" "socket run --spawn failure"
assert_not_contains "$ASSERT_OUTPUT" "missing-primary-disk" "socket run --spawn failure"

echo "PASS: Fast Mode readiness CLI/socket integration smoke ($STORE)"
