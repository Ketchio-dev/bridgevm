# Native VMM userspace GIC active-priority registers — 2026-10-07

This checkpoint records one deterministic interrupt-controller repair. It does
not establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `0f998a43e20d904c0069cddabba5938cfbe03337`.

## Defect

The optional userspace GICv3 (`BRIDGEVM_USERSPACE_GIC=1`) kept
`ICC_AP0R<n>_EL1` and `ICC_AP1R<n>_EL1` as plain storage. Acknowledging an
interrupt through `ICC_IAR1_EL1` did not change them, so a guest saw all-zero
active priorities while `ICC_RPR_EL1` reported an active priority. Writing them
had no effect on the running priority. A guest that saves and restores the
active-priority state, or that clears it to recover, therefore saw an
interface that contradicted its own running priority.

## Repair

The registers are now a view of the per-CPU acknowledged-interrupt list that
already drives the running priority. Group 1 bit *n* of the 128-bit view is
group priority 2*n*. This is QEMU's layout for seven preemption bits at the
minimum binary point. Group 0 is never acknowledged by this model, so its
registers read zero.

A write that clears a reported bit drops that priority without deactivating
the interrupt, and the CPU's IRQ line is re-evaluated. Writing back the value
last read changes nothing. A set bit with no acknowledged interrupt behind it
is ignored, so the guest cannot fabricate an active priority.

The contract comes from [Arm IHI0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269)
§4.8.3, §12.2.2 and the Appendix B `GetHighestActivePriority`/`PriorityDrop`
helpers. The register encoding is IMPLEMENTATION DEFINED; the architecture
requires only that zero means no active priority. Only the
last-read-value and zero writes have defined results, and those are the two
the fixtures check.

## Verification

The frozen baseline fixture from the earlier reproduction (sha256 prefix
`e6fcaf04f170f0a8`, 71 PASS / 1 FAIL on the old source) is carried unchanged.
Three new fixtures cover:

- nested activation and group 0 idle readback;
- split EOI before deactivation;
- write-back preservation, and clearing to zero unmasking a pending SPI while
  the interrupt stays active.

All use public system-register, MMIO and interrupt-line APIs. With the same
four fixtures, the old implementation produced **79 PASS / 4 FAIL** in the
userspace GIC suite and the repair produced **83 PASS / 0 FAIL**.

`userspace_gic/mod.rs` falls from 623 to 592 counted lines and
`cpu_interface.rs` from 40 to 38. The new production and test modules are
registered at their actual sizes. No existing ceiling increases.

## Boundary

This does not cover coarse-BPR priority capture, BPR changes while active,
Group 0 delivery, checkpoint capture of these registers, or behaviour of the
default Apple in-kernel GIC provider, which owns its own active-priority
state. No guest-visible deviation is added: QEMU `virt` also derives these
registers from acknowledgement and priority drop.
