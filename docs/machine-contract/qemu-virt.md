# The QEMU `virt`-compatible guest contract

Document status: **Current**
Last reviewed: **2026-10-02**

BridgeVM's Windows HVF engine presents a **QEMU `virt`-compatible guest
contract with documented deviations**. That phrasing is deliberate and replaces
the earlier claim that the guest sees a "bit-identical" platform.

## What the contract guarantees

The memory map, interrupt numbers, and device layout are declared by BridgeVM in
[`machine.rs`](../../crates/bridgevm-hvf/src/machine.rs) and summarized in the
independently maintained
[`BridgeVM virt platform register map`](../reference/bridgevm-virt-platform.md).
Device behavior is implemented from the public Arm, PCI, TCG, UEFI, ACPI,
Devicetree, and OASIS virtio interfaces named there, so:

- stock `edk2-aarch64-code.fd` boots unmodified;
- the firmware generates ACPI from the standard table flow;
- the same Windows 11 ARM media that installs under QEMU installs here;
- the separately implemented QEMU Compatibility Engine remains available as a
  user-selectable fallback, not as source material for the HVF implementation.

`crates/bridgevm-hvf/src/machine.rs` is the single source of truth for the
addresses and interrupt numbers themselves.

## Why it is not bit-identical

Some differences are unavoidable because the substrate is Apple's hypervisor
rather than QEMU's userspace models, and others are current defects. Claiming
bit-identity would hide both. The machine-readable list is
[`qemu-virt-deviations.json`](qemu-virt-deviations.json), combining inline entries
with flat `deviation_modules` named `qemu-virt-deviations-*.json`, recording behaviours,
impact and evidence; referenced modules preserve the same schema and contract.

Deviations fall into four kinds:

- **Substrate deviations** that will not go away, such as Apple's in-kernel GIC.
  Timer interrupts are delivered without a guest exit, which is why exit-count
  telemetry cannot see them.
- **Scope deviations** that describe what the product deliberately supports
  today, such as the experimental Vulkan path and the experimental
  D3D11-compatible subset.
- **Historical defect deviations** whose corrections remain recorded: the
  former deterministic SMCCC TRNG (A12) and incorrect PSCI state table (A13).
  Their current criterion states and evidence come from the capability registry.
- **Recovery deviations** for malformed split queues: BridgeVM refuses invalid
  addresses but does not reproduce QEMU's broken-device/NEEDS_RESET transition.

A defect deviation is never an excuse. It is recorded here so the contract stays
honest through correction, with criterion states and evidence tracked in [`capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json).

## Changing the contract

Adding a device or changing guest-visible behavior requires preserving the
declared compatibility contract or adding a deviation entry with evidence.
`scripts/check-contract-schema-json.py` validates JSON and referenced modules,
including metadata, visibility fields and unique IDs, in local and hosted checks.
