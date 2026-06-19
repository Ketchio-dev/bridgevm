#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-mmio-block-queue-live.XXXXXX")"
DISK="$STORE/file-backed-live-block.img"
WRITABLE_DISK="$STORE/writable-file-backed-live-block.img"
ISO="$STORE/installer-live-block.iso"
SECTOR="$STORE/sector-7.bin"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE=1 to emulate VirtIO-MMIO block queue/config/address/notify registers through the MMIO bus"
  exit 0
fi

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

if [[ -n "${BRIDGEVM_LIVE_HVF_RUNNER:-}" ]]; then
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh --verify-only "$BRIDGEVM_LIVE_HVF_RUNNER")" \
    || fail "configured HVF runner is not signed with the hypervisor entitlement"
else
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh)" \
    || fail "could not build/sign hvf-runner with the hypervisor entitlement"
fi

dd if=/dev/zero of="$DISK" bs=512 count=16 2>/dev/null
: >"$SECTOR"
for ((i = 0; i < 512; i++)); do
  value=$(((0xa0 + i) & 0xff))
  printf "\\$(printf '%03o' "$value")" >>"$SECTOR"
done
dd if="$SECTOR" of="$DISK" bs=512 seek=7 conv=notrunc 2>/dev/null
cp "$DISK" "$WRITABLE_DISK"

dd if=/dev/zero of="$ISO" bs=512 count=16 2>/dev/null
: >"$SECTOR"
for ((i = 0; i < 512; i++)); do
  value=$(((0xc0 + i) & 0xff))
  printf "\\$(printf '%03o' "$value")" >>"$SECTOR"
done
dd if="$SECTOR" of="$ISO" bs=512 seek=7 conv=notrunc 2>/dev/null

output="$("$runner" --mmio-block-queue-probe --allow-device 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --allow-device failed: $output"

assert_contains "$output" "HVF MMIO block queue/config/address/notify probe" "HVF MMIO block queue live output"
assert_contains "$output" "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton; VirtIO-MMIO block queue/config/address/notify skeleton" "HVF MMIO block queue live output"
assert_contains "$output" "Allowed: true" "HVF MMIO block queue live output"
assert_contains "$output" "Attempted: true" "HVF MMIO block queue live output"
assert_contains "$output" "Device bus created: true" "HVF MMIO block queue live output"
assert_contains "$output" "Device bus device count: 3" "HVF MMIO block queue live output"
assert_contains "$output" "read device_features at 0x50002010: expected=0x0, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "write driver_features at 0x50002020: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "write status_ack at 0x50002070: expected=not observed, write=0x1, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x1" "HVF MMIO block queue live output"
assert_contains "$output" "write status_driver at 0x50002070: expected=not observed, write=0x3, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x3" "HVF MMIO block queue live output"
assert_contains "$output" "write status_features_ok at 0x50002070: expected=not observed, write=0xb, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0xb" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_select at 0x50002030: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "read queue_num_max at 0x50002034: expected=0x80, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_num at 0x50002038: expected=not observed, write=0x8, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x8" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_desc_low at 0x50002080: expected=not observed, write=0x40001000, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x40001000" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_desc_high at 0x50002084: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_driver_low at 0x50002090: expected=not observed, write=0x40002000, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x40002000" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_driver_high at 0x50002094: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_device_low at 0x500020a0: expected=not observed, write=0x40003000, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x40003000" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_device_high at 0x500020a4: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_ready at 0x50002044: expected=not observed, write=0x1, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x1" "HVF MMIO block queue live output"
assert_contains "$output" "write status_driver_ok at 0x50002070: expected=not observed, write=0xf, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0xf" "HVF MMIO block queue live output"
assert_contains "$output" "read status at 0x50002070: expected=0xf, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "write queue_notify at 0x50002050: expected=not observed, write=0x0, run=true, address_set=true, write_value_set=true, exit=true, handled=true, injected=false, write_accepted=true, pc_advanced=true, captured=0x0" "HVF MMIO block queue live output"
assert_contains "$output" "read queue_ready at 0x50002044: expected=0x1, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read queue_desc_low at 0x50002080: expected=0x40001000, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read queue_driver_low at 0x50002090: expected=0x40002000, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read queue_device_low at 0x500020a0: expected=0x40003000, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read interrupt_status at 0x50002060: expected=0x1, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read config_generation at 0x500020fc: expected=0x0, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read capacity_low at 0x50002100: expected=0x4000, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "read capacity_high at 0x50002104: expected=0x0, write=not observed, run=true, address_set=true, write_value_set=false, exit=true, handled=true, injected=true, write_accepted=false, pc_advanced=true, captured=not observed" "HVF MMIO block queue live output"
assert_contains "$output" "Continuation exit observed: true" "HVF MMIO block queue live output"
assert_contains "$output" "Capacity high value preserved: true" "HVF MMIO block queue live output"
assert_contains "$output" "Continuation exit syndrome: 0x5a000000" "HVF MMIO block queue live output"
assert_contains "$output" "Capacity high after continue: 0x0" "HVF MMIO block queue live output"
assert_contains "$output" "Interrupt status value: 0x1" "HVF MMIO block queue live output"
assert_contains "$output" "Block backing kind: synthetic-sector-pattern" "HVF MMIO block queue live output"
assert_contains "$output" "Block backing path: not observed" "HVF MMIO block queue live output"
assert_contains "$output" "Request ring seeded: true" "HVF MMIO block queue live output"
assert_contains "$output" "Request completed after notify: true" "HVF MMIO block queue live output"
assert_contains "$output" "Request descriptor index: 0x0" "HVF MMIO block queue live output"
assert_contains "$output" "Request sector: 0x7" "HVF MMIO block queue live output"
assert_contains "$output" "Request byte offset: 0xe00" "HVF MMIO block queue live output"
assert_contains "$output" "Request data bytes: 0x200" "HVF MMIO block queue live output"
assert_contains "$output" "Request data prefix: 0x0708090a0b0c0d0e" "HVF MMIO block queue live output"
assert_contains "$output" "Request status byte: 0x0" "HVF MMIO block queue live output"
assert_contains "$output" "Request used index: 0x1" "HVF MMIO block queue live output"
assert_contains "$output" "Request used length: 0x201" "HVF MMIO block queue live output"
assert_contains "$output" "Request interrupt status: 0x1" "HVF MMIO block queue live output"
assert_contains "$output" "Blockers: none" "HVF MMIO block queue live output"

file_output="$("$runner" --mmio-block-queue-probe --allow-device --disk "$DISK" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --allow-device --disk failed: $file_output"

assert_contains "$file_output" "HVF MMIO block queue/config/address/notify probe" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Allowed: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Attempted: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Device bus created: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Continuation exit observed: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Block backing kind: host-file" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Block backing path: $DISK" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request ring seeded: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request completed after notify: true" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request descriptor index: 0x0" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request sector: 0x7" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request byte offset: 0xe00" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request data bytes: 0x200" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request data prefix: 0xa0a1a2a3a4a5a6a7" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request status byte: 0x0" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request used index: 0x1" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request used length: 0x201" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Request interrupt status: 0x1" "HVF MMIO block queue live file output"
assert_contains "$file_output" "Blockers: none" "HVF MMIO block queue live file output"

writable_output="$("$runner" --mmio-block-queue-probe --allow-device --writable-disk "$WRITABLE_DISK" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --allow-device --writable-disk failed: $writable_output"

assert_contains "$writable_output" "HVF MMIO block queue/config/address/notify probe" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Allowed: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Attempted: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Device bus created: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Continuation exit observed: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Block backing kind: host-file-writable" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Block backing path: $WRITABLE_DISK" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request ring seeded: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request completed after notify: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request descriptor index: 0x0" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request sector: 0x7" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request byte offset: 0xe00" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request data bytes: 0x200" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request data prefix: 0xa0a1a2a3a4a5a6a7" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request status byte: 0x0" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request used index: 0x1" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request used length: 0x201" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Request interrupt status: 0x1" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write completed after notify: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write request type: 0x1" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write sector: 0x7" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write byte offset: 0xe00" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write data bytes: 0x200" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write data prefix: 0xe0e1e2e3e4e5e6e7" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write status byte: 0x0" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write used index: 0x2" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Write used length: 0x1" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Flush completed after notify: true" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Flush request type: 0x4" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Flush status byte: 0x0" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Flush used index: 0x3" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Flush used length: 0x1" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Persisted data prefix: 0xe0e1e2e3e4e5e6e7" "HVF MMIO block queue live writable output"
assert_contains "$writable_output" "Blockers: none" "HVF MMIO block queue live writable output"

persisted_prefix="$(dd if="$WRITABLE_DISK" bs=1 skip=$((7 * 512)) count=8 2>/dev/null | od -An -tx1 -v | tr -d ' \n')"
[[ "$persisted_prefix" == "e0e1e2e3e4e5e6e7" ]] \
  || fail "writable live disk sector prefix was not persisted; got $persisted_prefix"

iso_output="$("$runner" --mmio-block-queue-probe --allow-device --iso "$ISO" 2>&1)" \
  || fail "hvf-runner --mmio-block-queue-probe --allow-device --iso failed: $iso_output"

assert_contains "$iso_output" "HVF MMIO block queue/config/address/notify probe" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Allowed: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Attempted: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Device bus created: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Continuation exit observed: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Block backing kind: host-iso-readonly" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Block backing path: $ISO" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request ring seeded: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request completed after notify: true" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request descriptor index: 0x0" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request sector: 0x7" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request byte offset: 0xe00" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request data bytes: 0x200" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request data prefix: 0xc0c1c2c3c4c5c6c7" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request status byte: 0x0" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request used index: 0x1" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request used length: 0x201" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Request interrupt status: 0x1" "HVF MMIO block queue live ISO output"
assert_contains "$iso_output" "Blockers: none" "HVF MMIO block queue live ISO output"

echo "PASS: HVF MMIO block queue live opt-in smoke"
