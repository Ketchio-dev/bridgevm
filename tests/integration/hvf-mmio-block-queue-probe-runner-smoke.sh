#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-mmio-block-queue-probe-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO block queue probe runner smoke: $(basename "$0")" >&2
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
unset BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE

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

output="$(cargo run -q -p hvf-runner -- --mmio-block-queue-probe 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe command failed: $output"

assert_contains "$output" "HVF MMIO block queue/config/address/notify probe" "HVF MMIO block queue runner output"
assert_contains "$output" "QEMU: not used" "HVF MMIO block queue runner output"
assert_contains "$output" "Apple VZ: not used" "HVF MMIO block queue runner output"
assert_contains "$output" "Guest execution: VirtIO-MMIO feature, queue, ring address, notify, status, and capacity registers, then HVC" "HVF MMIO block queue runner output"
assert_contains "$output" "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton; VirtIO-MMIO block queue/config/address/notify skeleton" "HVF MMIO block queue runner output"
assert_contains "$output" "Allowed: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Attempted: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Device bus created: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Device bus device count: 0" "HVF MMIO block queue runner output"
assert_contains "$output" "VirtIO-MMIO block queue/config steps:" "HVF MMIO block queue runner output"
assert_contains "$output" "read device_features at 0x50002010: expected=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "write driver_features at 0x50002020: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "write status_ack at 0x50002070: expected=not observed, write=0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "write status_driver at 0x50002070: expected=not observed, write=0x3" "HVF MMIO block queue runner output"
assert_contains "$output" "write status_features_ok at 0x50002070: expected=not observed, write=0xb" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_select at 0x50002030: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "read queue_num_max at 0x50002034: expected=0x80" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_num at 0x50002038: expected=not observed, write=0x8" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_desc_low at 0x50002080: expected=not observed, write=0x40001000" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_desc_high at 0x50002084: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_driver_low at 0x50002090: expected=not observed, write=0x40002000" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_driver_high at 0x50002094: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_device_low at 0x500020a0: expected=not observed, write=0x40003000" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_device_high at 0x500020a4: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_ready at 0x50002044: expected=not observed, write=0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "write status_driver_ok at 0x50002070: expected=not observed, write=0xf" "HVF MMIO block queue runner output"
assert_contains "$output" "read status at 0x50002070: expected=0xf" "HVF MMIO block queue runner output"
assert_contains "$output" "write queue_notify at 0x50002050: expected=not observed, write=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "read queue_ready at 0x50002044: expected=0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "read queue_desc_low at 0x50002080: expected=0x40001000" "HVF MMIO block queue runner output"
assert_contains "$output" "read queue_driver_low at 0x50002090: expected=0x40002000" "HVF MMIO block queue runner output"
assert_contains "$output" "read queue_device_low at 0x500020a0: expected=0x40003000" "HVF MMIO block queue runner output"
assert_contains "$output" "read interrupt_status at 0x50002060: expected=0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "read config_generation at 0x500020fc: expected=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "read capacity_low at 0x50002100: expected=0x4000" "HVF MMIO block queue runner output"
assert_contains "$output" "read capacity_high at 0x50002104: expected=0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "Continuation exit observed: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Capacity high value preserved: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Block IPA: 0x50002000" "HVF MMIO block queue runner output"
assert_contains "$output" "Instructions: LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue num max value: 0x80" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue num value: 0x8" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue ready value: 0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue descriptor address: 0x40001000" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue driver address: 0x40002000" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue device address: 0x40003000" "HVF MMIO block queue runner output"
assert_contains "$output" "Queue notify value: 0x0" "HVF MMIO block queue runner output"
assert_contains "$output" "Interrupt status value: 0x1" "HVF MMIO block queue runner output"
assert_contains "$output" "Block backing kind: synthetic-sector-pattern" "HVF MMIO block queue runner output"
assert_contains "$output" "Block backing path: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request ring seeded: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Request completed after notify: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Request descriptor index: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request sector: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request byte offset: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request data bytes: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request data prefix: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request status byte: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request used index: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request used length: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Request interrupt status: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write completed after notify: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Write request type: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write sector: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write byte offset: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write data bytes: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write data prefix: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write status byte: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write used index: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Write used length: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Flush completed after notify: false" "HVF MMIO block queue runner output"
assert_contains "$output" "Flush request type: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Flush status byte: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Flush used index: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Flush used length: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Persisted data prefix: not observed" "HVF MMIO block queue runner output"
assert_contains "$output" "Status value: 0xf" "HVF MMIO block queue runner output"
assert_contains "$output" "Capacity sectors: 0x4000" "HVF MMIO block queue runner output"
assert_not_contains "$output" "qemu-system" "HVF MMIO block queue runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF MMIO block queue runner output"
assert_no_backend_launch

DISK="$STORE/not-opened-live-block.img"
disk_output="$(cargo run -q -p hvf-runner -- --mmio-block-queue-probe --disk "$DISK" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --disk command failed: $disk_output"

assert_contains "$disk_output" "Allowed: false" "HVF MMIO block queue runner disk output"
assert_contains "$disk_output" "Attempted: false" "HVF MMIO block queue runner disk output"
assert_contains "$disk_output" "Block backing kind: host-file" "HVF MMIO block queue runner disk output"
assert_contains "$disk_output" "Block backing path: $DISK" "HVF MMIO block queue runner disk output"
assert_contains "$disk_output" "Request completed after notify: false" "HVF MMIO block queue runner disk output"
assert_contains "$disk_output" "Request byte offset: not observed" "HVF MMIO block queue runner disk output"
assert_not_contains "$disk_output" "qemu-system" "HVF MMIO block queue runner disk output"
assert_not_matches "$disk_output" '[0-9]+([.][0-9]+)?%' "HVF MMIO block queue runner disk output"
assert_no_backend_launch

ISO="$STORE/not-opened-installer.iso"
iso_output="$(cargo run -q -p hvf-runner -- --mmio-block-queue-probe --iso "$ISO" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --iso command failed: $iso_output"

assert_contains "$iso_output" "Allowed: false" "HVF MMIO block queue runner ISO output"
assert_contains "$iso_output" "Attempted: false" "HVF MMIO block queue runner ISO output"
assert_contains "$iso_output" "Block backing kind: host-iso-readonly" "HVF MMIO block queue runner ISO output"
assert_contains "$iso_output" "Block backing path: $ISO" "HVF MMIO block queue runner ISO output"
assert_contains "$iso_output" "Request completed after notify: false" "HVF MMIO block queue runner ISO output"
assert_contains "$iso_output" "Request byte offset: not observed" "HVF MMIO block queue runner ISO output"
assert_not_contains "$iso_output" "qemu-system" "HVF MMIO block queue runner ISO output"
assert_not_matches "$iso_output" '[0-9]+([.][0-9]+)?%' "HVF MMIO block queue runner ISO output"
assert_no_backend_launch

WRITABLE_DISK="$STORE/not-opened-writable-live-block.img"
writable_output="$(cargo run -q -p hvf-runner -- --mmio-block-queue-probe --writable-disk "$WRITABLE_DISK" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --writable-disk command failed: $writable_output"

assert_contains "$writable_output" "Allowed: false" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Attempted: false" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Block backing kind: host-file-writable" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Block backing path: $WRITABLE_DISK" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Request completed after notify: false" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Write completed after notify: false" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Flush completed after notify: false" "HVF MMIO block queue runner writable output"
assert_contains "$writable_output" "Persisted data prefix: not observed" "HVF MMIO block queue runner writable output"
assert_not_contains "$writable_output" "qemu-system" "HVF MMIO block queue runner writable output"
assert_not_matches "$writable_output" '[0-9]+([.][0-9]+)?%' "HVF MMIO block queue runner writable output"
assert_no_backend_launch

echo "PASS: HVF MMIO block queue probe runner opt-in metadata smoke ($STORE)"
