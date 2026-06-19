#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-pflash-map-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
FIRMWARE="$STORE/AAVMF_CODE.fd"
VARS_TEMPLATE="$STORE/AAVMF_VARS.fd"
VARS="$STORE/win11-arm-vars.fd"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Windows Arm HVF pflash map smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

byte_octal() {
  printf '%03o' "$1"
}

write_bytes() {
  local path="$1"
  local offset="$2"
  local bytes="$3"
  printf '%b' "$bytes" | dd of="$path" bs=1 seek="$offset" conv=notrunc 2>/dev/null
}

write_byte() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal "$value")"
}

write_le16() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))"
}

write_le32() {
  local path="$1"
  local offset="$2"
  local value="$3"
  write_bytes "$path" "$offset" "\\$(byte_octal $((value & 0xff)))\\$(byte_octal $(((value >> 8) & 0xff)))\\$(byte_octal $(((value >> 16) & 0xff)))\\$(byte_octal $(((value >> 24) & 0xff)))"
}

write_le64() {
  local path="$1"
  local offset="$2"
  local value="$3"
  local bytes=""
  local shift
  for shift in 0 8 16 24 32 40 48 56; do
    bytes+="\\$(byte_octal $(((value >> shift) & 0xff)))"
  done
  write_bytes "$path" "$offset" "$bytes"
}

write_uefi_fv_fixture() {
  local path="$1"
  local size="$2"
  dd if=/dev/zero of="$path" bs="$size" count=1 2>/dev/null
  write_bytes "$path" 16 '\214\214\371\141\322\113\054\117\212\211\042\115\257\334\361\157'
  write_le64 "$path" 32 "$size"
  write_bytes "$path" 40 '_FVH'
  write_le32 "$path" 44 327423
  write_le16 "$path" 48 72
  write_le16 "$path" 52 0
  write_byte "$path" 54 0
  write_byte "$path" 55 2
  write_le32 "$path" 56 1
  write_le32 "$path" 60 "$size"
  write_le32 "$path" 64 0
  write_le32 "$path" 68 0

  local sum
  sum="$(od -An -tu2 -N72 -v "$path" | awk '{ for (i = 1; i <= NF; i++) s = (s + $i) % 65536 } END { print s + 0 }')"
  local checksum=$(((65536 - sum) % 65536))
  write_le16 "$path" 50 "$checksum"
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

assert_not_matches() {
  local haystack="$1"
  local regex="$2"
  local label="$3"
  if printf '%s\n' "$haystack" | grep -Eq "$regex"; then
    fail "$label unexpectedly matched /$regex/; got: $haystack"
  fi
}

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
}

write_uefi_fv_fixture "$FIRMWARE" 131072
write_uefi_fv_fixture "$VARS_TEMPLATE" 65536

output="$(cargo run -q -p bridgevm-cli -- hvf windows-pflash-map-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars 2>&1)" \
  || fail "bridgevm hvf windows-pflash-map-probe command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI pflash map probe" "Windows Arm pflash map CLI output"
assert_contains "$output" "QEMU: not used" "Windows Arm pflash map CLI output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm pflash map CLI output"
assert_contains "$output" "HVF: not entered" "Windows Arm pflash map CLI output"
assert_contains "$output" "Guest execution: not entered; AArch64 UEFI pflash slots loaded into memory images" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware path: $FIRMWARE" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars path: $VARS" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars created: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Pflash region: 0x8000000..0x10000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash loaded: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash name: code" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash path: $FIRMWARE" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash IPA range: 0x8000000..0xc000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash slot bytes: 0x4000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash source bytes: 0x20000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash copied bytes: 0x20000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash zero padding bytes: 0x3fe0000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash writable: false" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash prefix verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Firmware pflash padding zeroed: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash loaded: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash name: vars" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash path: $VARS" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash IPA range: 0xc000000..0x10000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash slot bytes: 0x4000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash source bytes: 0x10000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash zero padding bytes: 0x3ff0000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash writable: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash prefix verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Vars pflash padding zeroed: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Pflash slots non-overlapping: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Guest RAM overlap verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Device MMIO overlap verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm pflash map CLI output"
assert_contains "$output" "Planned reset vector IPA: 0x8000000" "Windows Arm pflash map CLI output"
assert_contains "$output" "Blockers: none" "Windows Arm pflash map CLI output"
assert_not_contains "$output" "qemu-system" "Windows Arm pflash map CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm pflash map CLI output"
[[ -f "$VARS" ]] || fail "expected mutable vars file to be created"
cmp -s "$VARS_TEMPLATE" "$VARS" || fail "created vars store does not match template"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF pflash map CLI smoke ($STORE)"
