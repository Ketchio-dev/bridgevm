#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-mmio-serial-device-probe-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO serial device probe smoke: $(basename "$0")" >&2
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

output="$(cargo run -q -p bridgevm-cli -- hvf mmio-serial-device-probe 2>&1)" \
  || fail "bridgevm hvf mmio-serial-device-probe command failed: $output"

assert_contains "$output" "HVF MMIO serial device probe" "HVF MMIO serial device CLI output"
assert_contains "$output" "QEMU: not used" "HVF MMIO serial device CLI output"
assert_contains "$output" "Apple VZ: not used" "HVF MMIO serial device CLI output"
assert_contains "$output" "Guest execution: STR data register, LDR status register, then HVC" "HVF MMIO serial device CLI output"
assert_contains "$output" "Device model: PL011 UART skeleton" "HVF MMIO serial device CLI output"
assert_contains "$output" "Allowed: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Attempted: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Device bus created: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Device bus device count: 0" "HVF MMIO serial device CLI output"
assert_contains "$output" "Write exit observed: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Write handled by device: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Write value captured: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Status exit observed: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Status handled by device: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Status value injected: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Continuation exit observed: false" "HVF MMIO serial device CLI output"
assert_contains "$output" "Instructions: STR X0, [X1]; LDR X0, [X2]; HVC #0" "HVF MMIO serial device CLI output"
assert_contains "$output" "Serial data IPA: 0x50000000" "HVF MMIO serial device CLI output"
assert_contains "$output" "Serial status IPA: 0x50000018" "HVF MMIO serial device CLI output"
assert_contains "$output" "Serial write value: 0x41" "HVF MMIO serial device CLI output"
assert_contains "$output" "Serial status value: 0x90" "HVF MMIO serial device CLI output"
assert_not_contains "$output" "qemu-system" "HVF MMIO serial device CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF MMIO serial device CLI output"
assert_no_backend_launch

echo "PASS: HVF MMIO serial device probe CLI opt-in metadata smoke ($STORE)"
