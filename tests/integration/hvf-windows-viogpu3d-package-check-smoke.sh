#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-package-check.XXXXXX")"
VIOGPU3D="$STORE/viogpu3d"
VIOGPU3D_1050="$STORE/viogpu3d-1050"
UNKNOWN="$STORE/unknown"
PROVENANCE_MANIFEST="$STORE/provenance-manifest.txt"

mkdir -p "$VIOGPU3D" "$VIOGPU3D_1050" "$UNKNOWN"

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
  local protocol_marker="$2"
  local pci_device_id="${3:-10F7}"

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\\VEN_1AF4&DEV_$pci_device_id

; BridgeVMProtocol=$protocol_marker
INF
  write_minimal_pe "$dir/viogpu3d.sys" 144 252
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
}

MANIFEST="$STORE/viogpu3d-manifest.txt"

write_package "$VIOGPU3D" virgl
write_package "$VIOGPU3D_1050" virgl 1050
write_package "$UNKNOWN" audited

output="$(
  VIOGPU3D_SOURCE_REPO=https://example.invalid/viogpu3d.git \
  VIOGPU3D_SOURCE_REF=deadbeef \
  VIOGPU3D_BUILD_ID=test-build \
  VIOGPU3D_SIGNING_CERT=test-cert \
    scripts/check-hvf-windows-viogpu3d-package.sh --manifest "$MANIFEST" "$VIOGPU3D" 2>&1
)" ||
  fail "viogpu3d package check failed: $output"

assert_contains "$output" "BridgeVM viogpu3d package check" "package check"
assert_contains "$output" "protocol=virgl" "package check"
assert_contains "$output" "protocol_source=auto" "package check"
assert_contains "$output" "hwids=PCI\\VEN_1AF4&DEV_10F7" "package check"
assert_contains "$output" "manifest=$MANIFEST" "package check"
assert_contains "$output" "PASS: viogpu3d package is injection-ready" "package check"
assert_file_contains "$MANIFEST" "BridgeVM viogpu3d package manifest" "package manifest"
assert_file_contains "$MANIFEST" "source_repo=https://example.invalid/viogpu3d.git" "package manifest"
assert_file_contains "$MANIFEST" "source_ref=deadbeef" "package manifest"
assert_file_contains "$MANIFEST" "build_id=test-build" "package manifest"
assert_file_contains "$MANIFEST" "signing_cert=test-cert" "package manifest"
assert_file_contains "$MANIFEST" "protocol=virgl" "package manifest"
assert_file_contains "$MANIFEST" "hwids=PCI\\VEN_1AF4&DEV_10F7" "package manifest"
assert_file_contains "$MANIFEST" $'file=sys\tsha256=' "package manifest"
assert_file_contains "$MANIFEST" "pe_machine=0xaa64" "package manifest"
assert_file_contains "$MANIFEST" $'file=cat\tsha256=' "package manifest"

unknown_output="$(scripts/check-hvf-windows-viogpu3d-package.sh "$UNKNOWN" 2>&1)" &&
  fail "unknown protocol package unexpectedly passed: $unknown_output"

assert_contains "$unknown_output" "could not identify viogpu3d protocol" "unknown package check"

cat >"$UNKNOWN/bridgevm-package-provenance.env" <<EOF
VIOGPU3D_SOURCE_REPO=https://example.invalid/provenance.git
VIOGPU3D_SOURCE_REF=feedface
VIOGPU3D_BUILD_ID=provenance-build
VIOGPU3D_SIGNING_CERT=provenance-cert
VIOGPU3D_PROTOCOL=virgl
VIOGPU3D_PCI_DEVICE_ID=10f7
EOF

provenance_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --manifest "$PROVENANCE_MANIFEST" "$UNKNOWN" 2>&1
)" || fail "provenance package check failed: $provenance_output"

assert_contains "$provenance_output" "provenance=$UNKNOWN/bridgevm-package-provenance.env" "provenance package check"
assert_contains "$provenance_output" "source_repo=https://example.invalid/provenance.git" "provenance package check"
assert_contains "$provenance_output" "source_ref=feedface" "provenance package check"
assert_contains "$provenance_output" "build_id=provenance-build" "provenance package check"
assert_contains "$provenance_output" "signing_cert=provenance-cert" "provenance package check"
assert_contains "$provenance_output" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "provenance package check"
assert_contains "$provenance_output" "protocol=virgl" "provenance package check"
assert_contains "$provenance_output" "protocol_source=provenance" "provenance package check"
assert_file_contains "$PROVENANCE_MANIFEST" "provenance=$UNKNOWN/bridgevm-package-provenance.env" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "source_repo=https://example.invalid/provenance.git" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "source_ref=feedface" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "protocol=virgl" "provenance manifest"

conflict_output="$(
  VIOGPU3D_PROTOCOL=venus scripts/check-hvf-windows-viogpu3d-package.sh "$VIOGPU3D" 2>&1
)" && fail "conflicting protocol override unexpectedly passed: $conflict_output"

assert_contains "$conflict_output" "conflicts with package scan protocol=virgl" "conflict package check"

override_output="$(
  VIOGPU3D_PROTOCOL=venus scripts/check-hvf-windows-viogpu3d-package.sh "$UNKNOWN" 2>&1
)" || fail "manual protocol override failed: $override_output"

assert_contains "$override_output" "protocol=venus" "override package check"
assert_contains "$override_output" "protocol_source=env" "override package check"

id1050_output="$(scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 1050 "$VIOGPU3D_1050" 2>&1)" ||
  fail "DEV_1050 package check failed: $id1050_output"

assert_contains "$id1050_output" "hwids=PCI\\VEN_1AF4&DEV_1050" "DEV_1050 package check"
assert_contains "$id1050_output" "expected_hwid=PCI\\VEN_1AF4&DEV_1050" "DEV_1050 package check"

id_mismatch_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 10f7 "$VIOGPU3D_1050" 2>&1
)" && fail "DEV_1050 package unexpectedly matched DEV_10F7: $id_mismatch_output"

assert_contains "$id_mismatch_output" "does not advertise expected PCI\\VEN_1AF4&DEV_10F7" "HWID mismatch package check"

echo "PASS: viogpu3d package check smoke ($STORE)"
