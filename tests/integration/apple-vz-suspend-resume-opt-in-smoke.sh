#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
  exit 0
}

# Opt-in only: this boots a real Apple VZ guest, saves its machine state, and
# restores it. It reuses the Apple VZ live-boot fixture/opt-in conventions.
[[ "${BRIDGEVM_LIVE_VZ_ALLOW_REAL_START:-}" == "1" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1 to run the Apple VZ suspend/resume smoke"
[[ -n "${BRIDGEVM_LIVE_VZ_KERNEL:-}" && -f "${BRIDGEVM_LIVE_VZ_KERNEL}" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_KERNEL (run tests/integration/prepare-apple-vz-debian-fixture.sh)"
[[ -n "${BRIDGEVM_LIVE_VZ_INITRD:-}" && -f "${BRIDGEVM_LIVE_VZ_INITRD}" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_INITRD to the Debian arm64 netboot initrd"
[[ -n "${BRIDGEVM_LIVE_VZ_RAW_DISK:-}" && -f "${BRIDGEVM_LIVE_VZ_RAW_DISK}" ]] || \
  skip "set BRIDGEVM_LIVE_VZ_RAW_DISK to a disposable raw disk image"
command -v cargo >/dev/null 2>&1 || skip "cargo is required"

STORE="$(mktemp -d "/tmp/bridgevm-suspend-resume.XXXXXX")"
VM_NAME="suspend-resume-vz"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-console=hvc0 priority=low}"
RUNNER_PID=""

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

cleanup() {
  # The resumed VM runs detached; stop anything bound to this store.
  pkill -f "$STORE" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

assert_state() {
  local expected="$1"
  local state_file="$BUNDLE/metadata/state.json"
  [[ -f "$state_file" ]] || fail "state.json missing: $state_file"
  grep -q "\"state\": \"$expected\"" "$state_file" || \
    fail "expected state '$expected'; got: $(cat "$state_file")"
}

echo "Building and ad-hoc signing AppleVzRunner..."
RUNNER="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)" || \
  fail "could not build/sign AppleVzRunner with the virtualization entitlement"
export BRIDGEVM_APPLE_VZ_RUNNER="$RUNNER"

# Create a Fast Mode linux-kernel VM, then point it at the supplied fixture with
# a raw primary disk (Apple VZ requires raw) and a serial console command line.
bridgevm create "$VM_NAME" --os ubuntu --version 22.04 --arch arm64 --mode fast \
  --boot-mode linux-kernel --kernel-path boot/vmlinuz --initrd-path boot/initrd >/dev/null

perl -0pi -e 's#path: disks/root\.qcow2\n    size: 80GiB\n    format: qcow2#path: disks/root.raw\n    size: 64MiB\n    format: raw#' \
  "$BUNDLE/manifest.yaml"
perl -0pi -e "s#  initrdPath: boot/initrd#  initrdPath: boot/initrd\n  kernelCommandLine: \"$KERNEL_CMDLINE\"#" \
  "$BUNDLE/manifest.yaml"
rm -f "$BUNDLE/metadata/active-disk.json"

mkdir -p "$BUNDLE/boot" "$BUNDLE/disks"
cp "$BRIDGEVM_LIVE_VZ_KERNEL" "$BUNDLE/boot/vmlinuz"
cp "$BRIDGEVM_LIVE_VZ_INITRD" "$BUNDLE/boot/initrd"
cp "$BRIDGEVM_LIVE_VZ_RAW_DISK" "$BUNDLE/disks/root.raw"
chmod u+rw "$BUNDLE/disks/root.raw"

echo "Suspending (boot -> pause -> save machine state)..."
suspend_output="$(bridgevm suspend "$VM_NAME")" || fail "bridgevm suspend failed: $suspend_output"
assert_state "suspended"
state_file="$BUNDLE/metadata/suspend-images/$VM_NAME.bin"
[[ -s "$state_file" ]] || fail "saved machine state missing/empty: $state_file"
echo "Saved machine state: $state_file ($(wc -c <"$state_file" | tr -d ' ') bytes)"

echo "Resuming (restore -> run)..."
resume_output="$(bridgevm resume "$VM_NAME")" || fail "bridgevm resume failed: $resume_output"
assert_state "running"

echo "PASS: Apple VZ suspend/resume opt-in smoke ($STORE)"
echo "  suspended -> saved state -> resumed to running"
