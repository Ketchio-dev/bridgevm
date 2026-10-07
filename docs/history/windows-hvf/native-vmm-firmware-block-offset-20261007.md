# Native VMM firmware block-offset underflow — 2026-10-07

This checkpoint records one deterministic diagnostic repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `ab3c61e0`.

## Defect

The UEFI firmware run-loop probe classifies every guest MMIO write it
handles. It calls `windows_arm_firmware_block_device_mmio_offset`, which
searches the virtio-mmio block devices for the written address. The helper
used `bool::then_some(ipa - device.base_ipa)`. `then_some` evaluates its
argument before it tests the condition, so a write below a device's base
subtracted past zero. The PL011 UART at `0x1000_0000` and the PL031 RTC at
`0x1000_1000` both sit below the first block device at `0x1000_2000`. Builds
keep overflow checks enabled, so the first firmware UART write panicked the
probe.

This is the same eager-evaluation pattern that was repaired for the virtio
PCI ISR window. The run loop is reached only from the diagnostic CLI command
`hvf windows-firmware-run-loop-probe`. The product VMM does not use it.

## Repair

The helper now computes `ipa.checked_sub(device.base_ipa)` and keeps the
offset only when it is below the device size. Every in-range address returns
the same offset as before.

## Verification

A new fixture asks for the offsets of the PL011 and PL031 addresses and
classifies a write to each. The original code panics on the first lookup with
`attempt to subtract with overflow`. With the repair, both return no device,
and neither write is reported as an interrupt-source change. The existing
queue-routing fixtures, which cover in-range block-device registers, still
pass.

## Boundary

Only the out-of-range lookup changed. No firmware boot or Windows run was
performed for this repair.
