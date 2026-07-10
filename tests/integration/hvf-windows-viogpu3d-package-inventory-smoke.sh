#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-inventory.XXXXXX")"
SCAN_ROOT="$STORE/scan-root"
OUT="$STORE/out"
GOOD="$SCAN_ROOT/good/driver"
GOOD1050="$SCAN_ROOT/good1050/driver"
UNREGISTERED="$SCAN_ROOT/unregistered/driver"
BAD="$SCAN_ROOT/bad/driver"
EMPTY="$STORE/empty-root"

mkdir -p "$GOOD" "$GOOD1050" "$UNREGISTERED" "$BAD" "$EMPTY"

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
%VirtioGpu3D.DeviceDesc% = VioGpu3D_Inst, PCI\\VEN_1AF4&DEV_$pci_device_id

; BridgeVMProtocol=$protocol
INF
  write_minimal_pe "$dir/viogpu3d.sys" "$machine_low" "$machine_high"
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
}

add_umd_payload() {
  local dir="$1"
  local registration="$2"
  local dll

  cat >>"$dir/viogpu3d.inf" <<'INF'

[DestinationDirs]
VioGpu3D_Files.Usermode=11

[VioGpu3D_Inst.NT]
CopyFiles=VioGpu3D_Files.Usermode
AddReg=VioGpu3D_DeviceSettings

[VioGpu3D_Files.Usermode]
viogpu_d3d10.dll,viogpu_d3d10_arm64.dll,,0
viogpu_wgl.dll,viogpu_wgl_arm64.dll,,0
INF
  if [[ "$registration" == "registered" ]]; then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_DeviceSettings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10
HKR,,OpenGLVersion,%REG_DWORD%,4096
HKR,,OpenGLFlags,%REG_DWORD%,3
INF
  fi
  for dll in viogpu_d3d10_arm64.dll viogpu_wgl_arm64.dll; do
    write_minimal_pe "$dir/$dll" 144 252
  done
}

write_package "$GOOD" venus 144 252
write_package "$GOOD1050" virgl 144 252 1050
write_package "$UNREGISTERED" virgl 144 252 1050
write_package "$BAD" virgl 144 206
add_umd_payload "$GOOD1050" registered
add_umd_payload "$UNREGISTERED" unregistered

output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$SCAN_ROOT" \
    --out-dir "$OUT" \
    --max-depth 6 2>&1
)" || fail "inventory scan failed: $output"

assert_contains "$output" "BridgeVM viogpu3d package inventory" "inventory output"
assert_contains "$output" "candidate_count=4" "inventory output"
assert_contains "$output" "ready_count=3" "inventory output"
assert_contains "$output" "render_candidate_count=1" "inventory output"
assert_contains "$output" "candidate=$GOOD" "inventory output"
assert_contains "$output" "candidate_status=ready" "inventory output"
assert_contains "$output" "candidate_protocol=venus" "inventory output"
assert_contains "$output" "candidate=$GOOD1050" "inventory output"
assert_contains "$output" "candidate_protocol=virgl" "inventory output"
assert_contains "$output" "candidate_capability=umd-registered" "inventory output"
assert_contains "$output" "candidate_render_candidate=true" "inventory output"
assert_contains "$output" "candidate=$UNREGISTERED" "inventory output"
assert_contains "$output" "candidate_capability=umd-payload-unregistered" "inventory output"
assert_contains "$output" "candidate_render_candidate=false" "inventory output"
assert_contains "$output" "candidate=$BAD" "inventory output"
assert_contains "$output" "candidate_status=rejected" "inventory output"
assert_contains "$output" "is not ARM64 PE" "inventory output"
assert_contains "$output" "PASS: injection-ready viogpu3d package found" "inventory output"
assert_contains "$output" "PASS: UMD-registered viogpu3d render candidate found" "inventory output"
assert_file_contains "$OUT/inventory.txt" "ready_count=3" "inventory file"
assert_file_contains "$OUT/inventory.txt" "render_candidate_count=1" "inventory file"
assert_file_contains "$OUT/candidates.txt" "$GOOD" "candidates file"
assert_file_contains "$OUT/candidates.txt" "$GOOD1050" "candidates file"
assert_file_contains "$OUT/candidates.txt" "$UNREGISTERED" "candidates file"
assert_file_contains "$OUT/candidates.txt" "$BAD" "candidates file"
grep -Fq $'file=sys\tsha256=' "$OUT"/*-manifest.txt ||
  fail "no generated manifest contains a sys sha256 entry"

required_render_output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$SCAN_ROOT" \
    --out-dir "$STORE/render-required-out" \
    --require-render-candidate 2>&1
)" || fail "inventory with a required render candidate failed: $required_render_output"

assert_contains "$required_render_output" "render_candidate_count=1" "required render inventory"

empty_output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$EMPTY" \
    --out-dir "$STORE/empty-out" \
    --require-found 2>&1
)" && fail "empty required inventory unexpectedly passed: $empty_output"

assert_contains "$empty_output" "candidate_count=0" "empty inventory"
assert_contains "$empty_output" "ready_count=0" "empty inventory"
assert_contains "$empty_output" "render_candidate_count=0" "empty inventory"
assert_contains "$empty_output" "no injection-ready viogpu3d package found" "empty inventory"

no_render_output="$(
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$GOOD" \
    --out-dir "$STORE/no-render-out" \
    --require-render-candidate 2>&1
)" && fail "KMD-only inventory unexpectedly passed the render-candidate requirement: $no_render_output"

assert_contains "$no_render_output" "ready_count=1" "KMD-only required render inventory"
assert_contains "$no_render_output" "render_candidate_count=0" "KMD-only required render inventory"
assert_contains "$no_render_output" "no UMD-registered viogpu3d render candidate found" "KMD-only required render inventory"

echo "PASS: viogpu3d package inventory smoke ($STORE)"
