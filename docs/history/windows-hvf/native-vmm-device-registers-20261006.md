# Native VMM RTC, running-priority and xHCI event-address repairs — 2026-10-06

This checkpoint records three deterministic device-model repairs. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `331d90e115ea19d225a875609bc0509f0468193f`.

## PL031 load readback and latched alarm status

The PL031 RTC load register returned zero after a guest wrote a new counter
value. Loading the counter also cleared a previously latched raw alarm, even
though the guest had not acknowledged it through the interrupt-clear register.

The device now retains the last load value separately from the current counter
and preserves already-latched alarm status across a load. Reset load readback
remains zero regardless of the initial host epoch. Interrupt masking and the
existing write-one-to-clear behavior remain unchanged.

Five new public-MMIO regression cases accompany four unchanged tests extracted
from the original module. With the same frozen fixture, the old implementation
produced **6 PASS / 3 FAIL**; the repaired implementation produced **9 PASS /
0 FAIL** in both Debug and Release. The failures were zero load readback and
masked/unmasked latched status lost by a subsequent load below the match value.

The contract comes from [Arm DDI0224C](https://documentation-service.arm.com/static/5fa13c81b1a7c5445f29021e?token=),
sections 3.2, 3.3.3 and 3.3.8. These checks do not prove counter wrap, alarm
scheduling, control-register enable/disable or platform RTC IRQ delivery.

## Optional userspace GIC running-priority readback

The optional userspace GIC returned raw odd active priorities through
`ICC_RPR_EL1`, including the subpriority bit. With eight implemented priority
bits, the non-idle register readback excludes bit zero. The repair masks that
bit before selecting the running priority; the idle value remains `0xff`.

Eight new fixtures use public system-register, MMIO and interrupt-line APIs.
They cover eligible odd/even priorities, nesting, split EOI/deactivation,
SGI/PPI/SPI, per-CPU state and repeated reads. Existing full-priority PMR and
same-group preemption assertions remain intact. The same fixture produced
**73 PASS / 6 FAIL** with the old implementation and **79 PASS / 0 FAIL** with
the repair in both Debug and Release.

The contract comes from [Arm IHI0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269),
section 12.2.19 and Appendix B. Exact numeric fixtures use minimum BPR1. This
repair does not settle coarse-BPR priority capture, changes to BPR while active,
APR save/restore coherence or behavior of the default Apple GIC provider.

## xHCI event-ring address overflow

The event ring segment table is guest RAM and is re-read on every event post.
A guest that rewrote the segment base near the top of the address space made
`segment_base + enqueue * 16` overflow. Release builds keep overflow checks, so
the VMM thread panicked instead of refusing the post. The address is now formed
with checked addition and rejected through the existing invalid-segment path,
before any guest-memory write.

Four public MMIO/guest-RAM fixtures cover rejection with and without a pending
interrupt, the existing non-overflowing unmapped-address refusal and a mapped
32-event wrap with cycle toggling. Each rejection must leave guest RAM, CRCR,
IMAN, USBSTS, ERDP and the last-event statistics unchanged and count exactly
one failed post; restoring a valid descriptor then completes the same command.
The same fixture produced **2 PASS / 2 FAIL** with the old implementation, both
failures `attempt to add with overflow`, and **4 PASS / 0 FAIL** with the
repair in Debug and Release.

The first fixture draft wrote the overflowing base as `u64::MAX & !63`, which
strict Clippy rejected as an identity operation. The frozen pair was rerun with
the numerically identical `!63`; both old and repaired results are unchanged.
This is malformed-descriptor robustness only. It does not prove ring-capacity
handling, ERST segment-size bounds, or any Windows USB input behavior.

## Integration and verification boundary

The PL031 file budget falls from 153 to 118; four new test modules are
registered at their actual counted sizes. `xhci/event.rs` and its test module
stay at their existing ceilings. No existing ceiling increases.

The combined `bridgevm-hvf` library passes **1328 tests, with zero failures and
one existing ignore**, in both Debug and Release. Workspace and `venus`
all-target Clippy, format, structural-budget and diff checks pass. These are
local deterministic results; full-project and exact-source hosted results are
recorded separately for the published source.

Neither repair changes the product state, criterion thresholds or known-defect
wording, and none introduces a new guest-platform deviation: QEMU's PL031 also
returns the last loaded value and leaves raw status to the clear register.
