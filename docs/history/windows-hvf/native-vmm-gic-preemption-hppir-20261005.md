# Userspace GIC preemption and pending visibility — 2026-10-05

Integrated source checkpoint `56f9dd10d2104030ed302958734344a20806afbf`
repairs binary-point preemption and its coupled pending-register readback in
the optional userspace GIC. Lower subpriority within the same priority group
could incorrectly preempt an active interrupt. Fixing that comparison alone
then hid an observable pending interrupt from `ICC_HPPIR1_EL1`.

## Architectural scope

[Arm IHI 0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269)
section 4.8.2, Table 4-9, and sections 4.8.4–4.8.5 distinguish Group 1 NS
preemption grouping from the full-priority PMR comparison. With `CBPR=0`,
BPR1 values 1 through 7 remove one through seven low priority bits from the
group comparison. Pending interrupts still use full priority for ordering.
The model advertises eight priority bits and a minimum BPR1 readback of one.

Section 12.2.12 and section 12.20, printed pages 12-841–12-842, distinguish
HPPIR observation from IAR acknowledgement: only IAR first checks whether an
interrupt can be signaled. Appendix B's pending-selection helper checks group
enablement without applying PMR or the running-priority preemption test.
The modeled CPU interface reports PMHE and CBPR as zero.

## Paired preemption experiment and failed intermediate conclusion

Eight unchanged new fixtures exercise BPR1 0/1, 4 and 7, same-group blocking,
higher-group preemption, full PMR comparison including equality, idle priority,
full subpriority ordering, CPU routing and split/combined EOI. The whole GIC
filter gives baseline 42 PASS / 4 FAIL, then 46 PASS / 0 FAIL in both debug
and release. All 38 preexisting test bodies are unchanged. Fixture SHA-256:
`54bc6066ae36b802c130a8ce3320186933105f83ee9791f2ae9a17d3ac112d37`.
Preemption baseline raw SHA-256:
`6648bddf771dadd2a478808662052e1f767220d7b6390bb8a5430210055acbf8`.

Those passing tests did not establish HPPIR correctness. Independent review
withheld integration because HPPIR shared the modified delivery threshold.
With BPR1=4, active INTID 60 at priority 0x4f and pending INTID 61 at priority
0x40, the intermediate implementation changed HPPIR from 61 to 1023. The
interrupt cannot preempt, but must remain observable. This failed intermediate
conclusion is retained; the preemption-only candidate was not integrated.

## Combined repair and dedicated pending-register proof

`cpu_interface.rs` extracts the existing CPU-interface implementation; only
its delivery threshold changes. It masks non-idle running priority by the
effective BPR1 value, keeps idle 0xff, and combines that boundary with full PMR.
`hppir.rs` separately checks CPU-interface Group 1 enable and selects the
highest pending candidate below 0xff. It preserves existing distributor,
individual, interrupt-group, route and active-state filters. HPPIR reads do
not acknowledge interrupts or alter the delivery line, running priority,
pending state or active state.

Nine unchanged HPPIR fixtures give 4 PASS / 5 FAIL on the original production
code, then 3 PASS / 6 FAIL on the preemption-only candidate. The sole additional
failure is the exact same-group visibility regression above. The combined
implementation passes all nine plus the eight preemption and 38 existing tests:
55 PASS / 0 FAIL in debug and 55 PASS / 0 FAIL in release.
HPPIR fixture SHA-256:
`1be8b18e2842848d2a870795feb81f1172e56aa71d00ac8af1a149c54444bee5`.
Retained raw SHA-256 values are:

- Original HPPIR baseline: `5f9acfaeda310c0e75e4879c3f7acb230aa0e7d49da298f926c82555d6673f34`.
- Preemption-only HPPIR baseline: `68d273b213b651f1ba49323e3f960a8990fa05635d852bad6f234864c5ce2c08`.
- Combined debug: `e036e16e10e9621e2b257601f3e7bd33f052c86a77c4261104338eb3ff109be9`.
- Combined release: `d06cff34e52be71a99cf2e8e385fb2625d0648fa6c29c2970fcfeccebdca746d`.

The HPPIR cases also cover masked SPI/SGI/PPI visibility, repeated reads,
interface/global/individual/group disables, routing, active-pending exclusion
and full-priority ordering. Independent review authenticates the same fixture
bytes and the original PASS → intermediate FAIL → combined PASS transition.
Focused Clippy, formatting, staged budgets and whitespace checks pass.
The production ceiling decreases from 713 to 673; new CPU-interface/HPPIR
helpers and preemption/HPPIR fixtures have actual ceilings 40/11/161/194.

## Boundaries and incomplete integration evidence

Priority 0xff returning 1023 remains the existing behavior and an unchanged
negative control, not a resolved architectural visibility claim. RPR odd-value
readback, APR coherence, activation capture, CBPR and BPR changes while active
remain separate unresolved work. Fixtures configure BPR before activation.
There is no VM, Windows workload or physical interrupt result for this repair.

The parent local project check remains FAILED at 37 PASS / 8 FAIL. The parent
hosted campaign is FAILED/INCOMPLETE: runner-acquisition failure was verified,
and no actual full-project result was available at this checkpoint. Complete
successor project and exact-SHA hosted checks are still required. Criterion
states, thresholds, known defects and product wording are unchanged; no live
criterion or release promotion is claimed.
