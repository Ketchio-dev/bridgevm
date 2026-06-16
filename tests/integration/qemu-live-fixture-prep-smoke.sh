#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

HELPER="tests/integration/prepare-qemu-live-fixture.sh"

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

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-qemu-live-prep.XXXXXX")"
cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

QCOW2="$WORKDIR/root.qcow2"
STORE="$WORKDIR/store"
EVIDENCE="$WORKDIR/evidence"
printf 'synthetic qcow2 fixture\n' >"$QCOW2"

dry_store="$WORKDIR/dry-store"
dry_output="$("$HELPER" --dry-run --store "$dry_store" --sentinel bridgevm-qemu-ready --timeout 7)"
assert_contains "$dry_output" "export BRIDGEVM_LIVE_QEMU_STORE=$dry_store" "QEMU prep dry-run output"
assert_contains "$dry_output" "export BRIDGEVM_LIVE_QEMU_ARCH=x86_64" "QEMU prep dry-run output"
assert_contains "$dry_output" "export BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=bridgevm-qemu-ready" "QEMU prep dry-run output"
assert_contains "$dry_output" "# export BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1" "QEMU prep dry-run output"
[[ ! -e "$dry_store" ]] || fail "dry-run created store directory: $dry_store"

missing_output="$("$HELPER" --qcow2 "$WORKDIR/missing.qcow2" --store "$STORE" 2>&1)" && \
  fail "QEMU prep unexpectedly accepted a missing qcow2 fixture: $missing_output"
assert_contains "$missing_output" "qcow2 fixture does not exist" "QEMU prep missing qcow2 output"

output="$(
  "$HELPER" \
    --qcow2 "$QCOW2" \
    --arch aarch64 \
    --store "$STORE" \
    --vm live-qemu-smoke \
    --evidence-dir "$EVIDENCE" \
    --sentinel bridgevm-qemu-ready \
    --timeout 9
)"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_STORE=$STORE" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_VM=live-qemu-smoke" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_ARCH=aarch64" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR=$EVIDENCE" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_QCOW2_DISK=$QCOW2" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=bridgevm-qemu-ready" "QEMU prep output"
assert_contains "$output" "export BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS=9" "QEMU prep output"
assert_contains "$output" "# export BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1" "QEMU prep output"
[[ -d "$STORE" ]] || fail "QEMU prep did not create store directory: $STORE"
[[ -d "$EVIDENCE" ]] || fail "QEMU prep did not create evidence directory: $EVIDENCE"

echo "PASS: QEMU live fixture prep metadata-safe smoke"
