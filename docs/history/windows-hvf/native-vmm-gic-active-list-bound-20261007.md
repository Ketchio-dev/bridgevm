# Native VMM GIC active-list bound — 2026-10-07

Integration starts from main `ef3f6fb5`.

## Defect

The userspace GICv3 CPU interface keeps an ordered list of acknowledged
interrupts. It derives the running priority and the `ICC_AP1Rn_EL1` view
from that list. An entry is pushed on `ICC_IAR1_EL1`. It is removed only by
the CPU interface deactivation path: `ICC_DIR_EL1`, or `ICC_EOIR1_EL1` with
`EOImode` clear.

With `ICC_CTLR_EL1.EOImode` set, a guest can priority-drop an interrupt with
`EOIR1` and then deactivate it through `GICD_ICACTIVER` or `GICR_ICACTIVER0`.
Those registers clear the distributor or redistributor active bit, but they
do not touch the CPU interface list. The interrupt can be acknowledged again
and pushes a new entry, while the dropped entry stays behind. A guest loop of
acknowledge, priority drop and distributor deactivation therefore grew host
memory without limit. Every acknowledgement and priority calculation also
scanned the growing list. A probe reached 100,000 entries after 100,000
iterations.

## Repair

The acknowledge path moved from `userspace_gic/mod.rs` to
`userspace_gic/acknowledge.rs`. Before it pushes a new entry, it removes any
priority-dropped entry for the same INTID. An interrupt is acknowledged only
while it is inactive, so a dropped entry for that INTID was already
deactivated. It holds no active priority, because running priority and APR
ignore dropped entries. Removing it changes no guest-visible register.

An entry that is still undropped keeps its priority until `EOIR1`, as before.
Each new undropped entry must preempt the running priority, so the undropped
entries are bounded by the 128 group-priority levels. Dropped entries only
come from undropped ones, and each acknowledgement of an INTID retires its
dropped entries. The list is therefore bounded by the 256-INTID space and the
priority levels, whatever the guest does.

`userspace_gic/mod.rs` drops from 592 to 565 lines.

## Evidence

`distributor_deactivation_does_not_accumulate_dropped_entries` runs 4096
iterations of acknowledge, drop and `GICD_ICACTIVER`. It checks that the list
holds one entry, that `RPR` reads idle, and that `ICC_DIR_EL1` empties the
list. With the `retain` removed, the test fails with 4096 entries.

`reacknowledgement_keeps_an_undropped_preempted_priority` checks that an
undropped earlier acknowledgement still sets `RPR` after the same INTID is
deactivated through the distributor and acknowledged again at a higher
priority.

`bridgevm-hvf` lib tests pass 1347/0 with one existing ignore, and clippy is
clean.

## Scope

Acknowledgement order, priority, preemption and the APR encoding are
unchanged for guests that deactivate through the CPU interface. A11 remains
OPEN. Capability states, thresholds, known defects and product wording are
unchanged, and there is no live Windows or release claim.
