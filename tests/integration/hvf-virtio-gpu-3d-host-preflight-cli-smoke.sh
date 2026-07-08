#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-virtio-gpu-3d-host-preflight.XXXXXX")"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
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
  cargo run -q -p bridgevm-cli -- \
    hvf virtio-gpu-3d-host-preflight 2>&1
)" || fail "virtio-gpu 3D host preflight failed: $output"

assert_contains "$output" "HVF virtio-gpu 3D host preflight" "host preflight output"
assert_contains "$output" "QEMU: not used" "host preflight output"
assert_contains "$output" "Guest execution: not entered" "host preflight output"
assert_contains "$output" "Renderer: synthetic host-visible blob backend" "host preflight output"
assert_contains "$output" "Requested protocol: venus" "host preflight output"
assert_contains "$output" "VENUS capset id 4: true" "host preflight output"
assert_contains "$output" "VENUS expected capset id 4: true" "host preflight output"
assert_contains "$output" "RESOURCE_MAP_BLOB OK: true" "host preflight output"
assert_contains "$output" "SHM map called: true" "host preflight output"
assert_contains "$output" "SUBMIT_3D OK: true" "host preflight output"
assert_contains "$output" "SUBMIT_3D bytes: 8" "host preflight output"
assert_contains "$output" "Fence completed: true" "host preflight output"
assert_contains "$output" "RESOURCE_UNMAP_BLOB OK: true" "host preflight output"
assert_contains "$output" "Blockers: none" "host preflight output"

virgl_output="$(
  cargo run -q -p bridgevm-cli -- \
    hvf virtio-gpu-3d-host-preflight --protocol virgl 2>&1
)" || fail "virtio-gpu 3D VirGL host preflight failed: $virgl_output"

assert_contains "$virgl_output" "Requested protocol: virgl" "VirGL host preflight output"
assert_contains "$virgl_output" "VIRGL capset id 1: true" "VirGL host preflight output"
assert_contains "$virgl_output" "VIRGL expected capset id 1: true" "VirGL host preflight output"
assert_contains "$virgl_output" "SUBMIT_3D OK: true" "VirGL host preflight output"
assert_contains "$virgl_output" "Blockers: none" "VirGL host preflight output"

echo "PASS: HVF virtio-gpu 3D host preflight CLI smoke ($STORE)"
