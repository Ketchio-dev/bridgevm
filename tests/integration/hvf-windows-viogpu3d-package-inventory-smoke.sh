#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-inventory.XXXXXX")"
SCAN_ROOT="$STORE/scan-root"
OUT="$STORE/out"
GOOD="$SCAN_ROOT/good/driver"
GOOD1050="$SCAN_ROOT/good1050/driver"
BAD="$SCAN_ROOT/bad/driver"
EMPTY="$STORE/empty-root"

mkdir -p "$GOOD" "$GOOD1050" "$BAD" "$EMPTY"

write_minimal_pe() {
  local path="$1"
  local machine_low_octal="$2"
  local machine_high_octal="$3"

  dd if=/dev/zero of="$path" bs=512 count=1 >/dev/null 2>&1
  printf 'MZ' | dd of="$path" bs=1 seek=0 conv=notrunc >/dev/null 2>&1
  printf '\200\000\000\000' | dd of="$path" bs=1 seek=60 conv=notrunc >/dev/null 2>&1
  printf "PE\000\000\\$machine_low_octal\\$machine_high_octal" |
    dd of="$path" bs=1 seek=128 conv=notrunc >/dev/null 2>&1
}

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

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label file missing: $path"
  grep -Fq "$needle" "$path" || fail "$label missing '$needle' in $path"
}

write_package() {
  local dir="$1"
  local protocol="$2"
  local machine_low="$3"
  local machine_high="$4"
  local pci_device_id="${5:-10F7}"

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\\VEN_1AF4&DEV_$pci_device_id

; BridgeVMProtocol=$protocol
INF
  write_minimal_pe "$dir/viogpu3d.sys" "$machine_low" "$machine_high"
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
}

write_package "$GOOD" venus 144 252
write_package "$GOOD1050" virgl 144 252 1050
write_package "$BAD" virgl 144 206

output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$SCAN_ROOT" \
    --out-dir "$OUT" \
    --max-depth 6 2>&1
)" || fail "inventory scan failed: $output"

assert_contains "$output" "BridgeVM viogpu3d package inventory" "inventory output"
assert_contains "$output" "candidate_count=3" "inventory output"
assert_contains "$output" "ready_count=2" "inventory output"
assert_contains "$output" "candidate=$GOOD" "inventory output"
assert_contains "$output" "candidate_status=ready" "inventory output"
assert_contains "$output" "candidate_protocol=venus" "inventory output"
assert_contains "$output" "candidate=$GOOD1050" "inventory output"
assert_contains "$output" "candidate_protocol=virgl" "inventory output"
assert_contains "$output" "candidate=$BAD" "inventory output"
assert_contains "$output" "candidate_status=rejected" "inventory output"
assert_contains "$output" "is not ARM64 PE" "inventory output"
assert_contains "$output" "PASS: injection-ready viogpu3d package found" "inventory output"
assert_file_contains "$OUT/inventory.txt" "ready_count=2" "inventory file"
assert_file_contains "$OUT/candidates.txt" "$GOOD" "candidates file"
assert_file_contains "$OUT/candidates.txt" "$GOOD1050" "candidates file"
assert_file_contains "$OUT/candidates.txt" "$BAD" "candidates file"
grep -Fq $'file=sys\tsha256=' "$OUT"/*-manifest.txt ||
  fail "no generated manifest contains a sys sha256 entry"

empty_output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$EMPTY" \
    --out-dir "$STORE/empty-out" \
    --require-found 2>&1
)" && fail "empty required inventory unexpectedly passed: $empty_output"

assert_contains "$empty_output" "candidate_count=0" "empty inventory"
assert_contains "$empty_output" "ready_count=0" "empty inventory"
assert_contains "$empty_output" "no injection-ready viogpu3d package found" "empty inventory"

echo "PASS: viogpu3d package inventory smoke ($STORE)"
