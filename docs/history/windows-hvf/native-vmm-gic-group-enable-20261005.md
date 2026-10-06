# Userspace GIC global Group 1 disable — 2026-10-05

Integrated source checkpoint `2c8923fcb9fbdf75470c2efa3d307b474bdc978e`
repairs global Group 1 masking in the optional userspace GIC. A pending SGI
or PPI could still signal and be acknowledged after the guest disabled its
group through `GICD_CTLR`; the old group check covered only SPIs.

## Contract and reproduced state

[Arm IHI 0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269)
sections 4.7.2–4.7.3, printed pages 4-66–4-67, exclude interrupts whose group
is disabled in `GICD_CTLR` from highest-priority selection, including private
interrupts controlled by `GICR_*`. A pending interrupt must be retrieved from
the CPU interface when its group enable changes from one to zero. This does
not require discarding pending state or preventing completion of an already
active interrupt.

Six identical deterministic fixtures cover SGIs, the virtual timer PPI,
a software-pending PPI, the existing SPI gate, subordinate individual/CPU
interface masks and split EOI of an active interrupt. They exercise both CPU
routes and disable values zero and Group-0-only where applicable. Whole-filter
R2 baseline results are 34 PASS / 4 FAIL: the three private-interrupt disable
cases and the split-EOI case incorrectly retain an asserted IRQ line. The
SPI and subordinate-mask controls pass.

## Repair, test setup and paired results

The existing three-line global Group 1 guard moves from SPI-only selection
to `highest_candidate`, shared by IRQ-line evaluation, `ICC_HPPIR1_EL1` and
`ICC_IAR1_EL1`. Existing `GICD_CTLR` writes still notify all CPUs. Pending and
active state, EOI processing, timer generation and the default Apple GIC path
are unchanged. This restores the architectural contract without adding an
intentional machine-contract deviation.

Existing test helpers move to `test_support.rs`. Both `wake_cpu` and
`enable_vtimer_ppi` explicitly enable global Group 1 before the paired R2
baseline and repaired runs. Every preexisting test body and assertion is
byte-identical. The test-file ceiling decreases from 387 to 343; new helper
and regression ceilings are their actual sizes, 49 and 170. The production
file ceiling remains 713.

The six new fixtures are byte-identical across both sides, SHA-256
`efa11ac49d6aa65a177b48e8a6e7de7252764f3c402c3a27447be93dd67b5369`.
The complete repaired userspace-GIC filter passes 38 tests in debug and 38 in
release, including all 32 existing tests. Retained raw SHA-256 values are:

- R2 baseline: `4124c6e8de021d3ef0498e937d9b8898e9eb91749946d799946cd7e1631da9c4`.
- R2 debug: `06468fd39643f8e931af4323388ddb7f30b42d7b987f5dcc4ce6cbaeca1d1a42`.
- R2 release: `f1d7829811cc1b80813a00e972200902751a5c8ab453ab6ab11cf7fe2cad0a79`.

The initial formatting failure was corrected before fixture freezing. R1
repaired debug and release each returned 37 PASS / 1 FAIL because the old
WAKER fixture bypassed `wake_cpu` and never enabled the global group. Release
R1 had been launched before that failed debug result was inspected. These
failed records remain retained; the explicit timer-helper setup correction
was then applied equally to both R2 sides. Focused Clippy, formatting, budgets
and whitespace checks pass. Independent source and paired-evidence review
found no blocker.

## Evidence boundary

These are deterministic model results, with no VM or Windows workload run
and no attribution of an observed live failure to this defect. The parent
local project check returned 37 PASS / 8 FAIL in the restricted environment;
those failures remain recorded. Complete successor project checks and
exact-SHA GitHub-hosted verification are still required. Criterion states,
thresholds, known defects and product wording are unchanged; no live criterion
or release promotion is claimed.
