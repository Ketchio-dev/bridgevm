#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-mmio-block-device-probe-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO block device probe smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"
unset BRIDGEVM_HVF_ALLOW_VM_CREATE
unset BRIDGEVM_HVF_ALLOW_VCPU_RUN
unset BRIDGEVM_HVF_ALLOW_MEMORY_MAP
unset BRIDGEVM_HVF_ALLOW_GUEST_ENTRY
unset BRIDGEVM_HVF_ALLOW_EXIT_LOOP
unset BRIDGEVM_HVF_ALLOW_MMIO_READ
unset BRIDGEVM_HVF_ALLOW_MMIO_EMULATION
unset BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION
unset BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE
unset BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE
unset BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE

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

output="$(cargo run -q -p bridgevm-cli -- hvf mmio-block-device-probe 2>&1)" \
  || fail "bridgevm hvf mmio-block-device-probe command failed: $output"

assert_contains "$output" "HVF MMIO block device probe" "HVF MMIO block device CLI output"
assert_contains "$output" "QEMU: not used" "HVF MMIO block device CLI output"
assert_contains "$output" "Apple VZ: not used" "HVF MMIO block device CLI output"
assert_contains "$output" "Guest execution: LDR W0 VirtIO-MMIO identity registers, then HVC" "HVF MMIO block device CLI output"
assert_contains "$output" "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton" "HVF MMIO block device CLI output"
assert_contains "$output" "Allowed: false" "HVF MMIO block device CLI output"
assert_contains "$output" "Attempted: false" "HVF MMIO block device CLI output"
assert_contains "$output" "Device bus created: false" "HVF MMIO block device CLI output"
assert_contains "$output" "Device bus device count: 0" "HVF MMIO block device CLI output"
assert_contains "$output" "VirtIO-MMIO block identity reads:" "HVF MMIO block device CLI output"
assert_contains "$output" "magic at 0x50002000: expected 0x74726976" "HVF MMIO block device CLI output"
assert_contains "$output" "version at 0x50002004: expected 0x2" "HVF MMIO block device CLI output"
assert_contains "$output" "device_id at 0x50002008: expected 0x2" "HVF MMIO block device CLI output"
assert_contains "$output" "vendor_id at 0x5000200c: expected 0x4252564d" "HVF MMIO block device CLI output"
assert_contains "$output" "Continuation exit observed: false" "HVF MMIO block device CLI output"
assert_contains "$output" "Vendor value preserved: false" "HVF MMIO block device CLI output"
assert_contains "$output" "Block IPA: 0x50002000" "HVF MMIO block device CLI output"
assert_contains "$output" "Instructions: LDR W0 magic/version/device/vendor; HVC #0" "HVF MMIO block device CLI output"
assert_contains "$output" "VirtIO magic value: 0x74726976" "HVF MMIO block device CLI output"
assert_contains "$output" "VirtIO version value: 0x2" "HVF MMIO block device CLI output"
assert_contains "$output" "VirtIO block device ID value: 0x2" "HVF MMIO block device CLI output"
assert_contains "$output" "VirtIO vendor ID value: 0x4252564d" "HVF MMIO block device CLI output"
assert_not_contains "$output" "qemu-system" "HVF MMIO block device CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF MMIO block device CLI output"
assert_no_backend_launch

echo "PASS: HVF MMIO block device probe CLI opt-in metadata smoke ($STORE)"
