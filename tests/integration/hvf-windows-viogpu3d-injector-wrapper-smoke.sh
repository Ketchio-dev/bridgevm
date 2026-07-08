#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-injector-wrapper.XXXXXX")"
VIOGPU3D="$STORE/viogpu3d"
NETKVM="$STORE/netkvm"
FAKE_INJECTOR="$STORE/fake-build-injector.sh"
LOG="$STORE/fake-build-injector.log"
OUT="$STORE/win-viogpu3d-injector.raw"
MANIFEST="$STORE/viogpu3d-manifest.txt"

mkdir -p "$VIOGPU3D" "$NETKVM"

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

cat >"$VIOGPU3D/viogpu3d.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=virgl
INF
write_minimal_pe "$VIOGPU3D/viogpu3d.sys" 144 252
printf 'fake catalog\n' >"$VIOGPU3D/viogpu3d.cat"

cat >"$NETKVM/netkvm.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64
INF
printf 'fake sys\n' >"$NETKVM/netkvm.sys"

cat >"$FAKE_INJECTOR" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
{
  printf 'ENABLE_TESTSIGNING=%s\n' "${ENABLE_TESTSIGNING:-}"
  printf 'DRIVER_DIRS=%s\n' "${DRIVER_DIRS:-}"
  printf 'OUT=%s\n' "${OUT:-}"
} >"${BRIDGEVM_FAKE_INJECTOR_LOG:?}"
SH
chmod +x "$FAKE_INJECTOR"

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

output="$(
  BRIDGEVM_FAKE_INJECTOR_LOG="$LOG" \
  BUILD_INJECTOR="$FAKE_INJECTOR" \
  VIOGPU3D_DIR="$VIOGPU3D" \
  VIOGPU3D_MANIFEST="$MANIFEST" \
  NETKVM_DIR="$NETKVM" \
  OUT="$OUT" \
    scripts/build-hvf-windows-viogpu3d-injector.sh 2>&1
)" || fail "viogpu3d injector wrapper failed: $output"

log="$(cat "$LOG")"
assert_contains "$output" "driver package: $VIOGPU3D" "wrapper output"
assert_contains "$output" "protocol=virgl" "wrapper output"
assert_contains "$output" "manifest=$MANIFEST" "wrapper output"
assert_contains "$output" "driver protocol: virgl" "wrapper output"
assert_contains "$output" "driver dirs: netkvm:$NETKVM viogpu3d:$VIOGPU3D" "wrapper output"
assert_contains "$log" "ENABLE_TESTSIGNING=1" "fake injector env"
assert_contains "$log" "DRIVER_DIRS=netkvm:$NETKVM viogpu3d:$VIOGPU3D" "fake injector env"
assert_contains "$log" "OUT=$OUT" "fake injector env"
assert_file_contains "$MANIFEST" "protocol=virgl" "wrapper manifest"
assert_file_contains "$MANIFEST" $'file=sys\tsha256=' "wrapper manifest"

BAD_VIOGPU3D="$STORE/bad-viogpu3d"
mkdir -p "$BAD_VIOGPU3D"
cp "$VIOGPU3D/viogpu3d.inf" "$BAD_VIOGPU3D/viogpu3d.inf"
printf 'fake catalog\n' >"$BAD_VIOGPU3D/viogpu3d.cat"
write_minimal_pe "$BAD_VIOGPU3D/viogpu3d.sys" 144 206

bad_output="$(
  BRIDGEVM_FAKE_INJECTOR_LOG="$STORE/bad-build.log" \
  BUILD_INJECTOR="$FAKE_INJECTOR" \
  VIOGPU3D_DIR="$BAD_VIOGPU3D" \
  NETKVM_DIR="$NETKVM" \
  OUT="$STORE/bad.raw" \
    scripts/build-hvf-windows-viogpu3d-injector.sh 2>&1
)" && fail "x64 viogpu3d package unexpectedly passed: $bad_output"

assert_contains "$bad_output" "is not ARM64 PE" "x64 negative-control output"
assert_contains "$bad_output" "machine=0x8664" "x64 negative-control output"

NO_CAT_VIOGPU3D="$STORE/no-cat-viogpu3d"
mkdir -p "$NO_CAT_VIOGPU3D"
cp "$VIOGPU3D/viogpu3d.inf" "$NO_CAT_VIOGPU3D/viogpu3d.inf"
write_minimal_pe "$NO_CAT_VIOGPU3D/viogpu3d.sys" 144 252

no_cat_output="$(
  BRIDGEVM_FAKE_INJECTOR_LOG="$STORE/no-cat-build.log" \
  BUILD_INJECTOR="$FAKE_INJECTOR" \
  VIOGPU3D_DIR="$NO_CAT_VIOGPU3D" \
  NETKVM_DIR="$NETKVM" \
  OUT="$STORE/no-cat.raw" \
    scripts/build-hvf-windows-viogpu3d-injector.sh 2>&1
)" && fail "catalog-less viogpu3d package unexpectedly passed: $no_cat_output"

assert_contains "$no_cat_output" "no .cat catalog found" "catalog negative-control output"

echo "PASS: viogpu3d injector wrapper smoke ($STORE)"
