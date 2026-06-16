#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

LIVE_SMOKE="tests/integration/apple-vz-live-boot-opt-in-smoke.sh"

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

output="$(
  env \
    -u BRIDGEVM_LIVE_VZ_ALLOW_REAL_START \
    -u BRIDGEVM_LIVE_VZ_KERNEL \
    -u BRIDGEVM_LIVE_VZ_INITRD \
    -u BRIDGEVM_LIVE_VZ_RAW_DISK \
    -u BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE \
    -u BRIDGEVM_LIVE_VZ_STOP_AFTER_SECONDS \
    -u BRIDGEVM_LIVE_VZ_FORCE_STOP_GRACE_SECONDS \
    -u BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED \
    -u BRIDGEVM_LIVE_VZ_MEMORY_MIB \
    -u BRIDGEVM_LIVE_VZ_CPU_COUNT \
    -u BRIDGEVM_LIVE_VZ_RUNNER \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully without live opt-in; got: $output"

assert_contains "$output" "SKIP:" "live opt-in default output"
assert_contains "$output" "BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1" "live opt-in default output"

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-live-vz-preflight.XXXXXX")"
cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

printf 'kernel\n' >"$WORKDIR/kernel"
printf 'disk\n' >"$WORKDIR/raw-disk"
ln -s "$WORKDIR/kernel" "$WORKDIR/kernel-link"

symlink_output="$(
  env \
    BRIDGEVM_LIVE_VZ_ALLOW_REAL_START=1 \
    BRIDGEVM_LIVE_VZ_KERNEL="$WORKDIR/kernel-link" \
    BRIDGEVM_LIVE_VZ_RAW_DISK="$WORKDIR/raw-disk" \
    BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED="login:" \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully for symlink preflight skip; got: $symlink_output"

assert_contains "$symlink_output" "SKIP:" "live opt-in symlink preflight output"
assert_contains "$symlink_output" "kernel fixture must not be a symlink" "live opt-in symlink preflight output"

echo "PASS: Apple VZ live opt-in smoke skips successfully by default"
