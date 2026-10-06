# NVMe admin response PRP placement — 2026-10-06

Integrated source `6dcd6fa32ec39df7a1e3b12e7b19568a0357034a` repairs the
existing Get Log Page and Security Receive discovery responses. A transfer
crossing a memory-page boundary previously wrote past PRP1 into adjacent
RAM, ignored nonadjacent PRP2 and could report success for invalid PRP2.

## Existing response boundary

[NVM Express 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
section 5.14, Figure 185 and section 5.25, Figure 333 use the common data
pointer definition in Figure 105. The tested existing payloads are SMART
and firmware logs of 512 bytes, the implemented command-effects response
of 4096 bytes, and the ten-byte security discovery response. Protocol 00h
discovery is independent of Security Send under section 5.25.2.

A shared admin-data helper contains the previously proven Identify PRP
transfer body byte for byte. Get Log Page and Security Receive call it for
their existing payloads; Identify also uses this helper. Reversing those
call and receiver changes restores all three handlers exactly. Payload
builders, validation order, unsupported requests and prior tests remain
unchanged. A later unmapped destination can leave an earlier intended span
written; this change makes no atomic DMA rollback promise.

APST is excluded: section 5.21.1.12 on page 216 requires its 256-byte
structure to be physically contiguous. Nonadjacent PRP2 is therefore not a
valid APST success oracle. Its existing contiguous response, PRP1-zero
behavior, result fields and unsupported feature policy are unchanged;
this is not a finding that all malformed APST requests are handled correctly.

The command-effects response remains implemented without changing its
existing Identify LPA advertisement. Valid log fixtures use zero log offset
and zero extended count. No optional feature or security support is added.

## Paired deterministic evidence

Seventeen corrected fixtures give 7 PASS / 10 FAIL against unchanged
production. Four separate split-response cases overwrite the adjacent-page
sentinel. Six command-family pointer cases incorrectly return success for
missing, misaligned or unmapped PRP2. Seven controls pass, covering aligned
and single-page responses, a four-byte log prefix, unsupported requests and
unmapped first destinations. Six pages of sentinels constrain DMA effects;
public SQ/CQ checks also cover status, CID, SQ head, SQ ID and phase.

Baseline loops stop at their first failure: offsets 3588 for 512-byte logs,
4 for command effects and 4088 for discovery. Later offsets are exercised
by repaired runs, not separately observed baseline failures. Invalid-log
pointer loops stop on SMART in baseline; repaired runs cover all three logs.

R1 also reported 7 PASS / 10 FAIL, with an unused import warning in the new
fixture. Its exact file and raw output remain retained. Only that import
was removed before repeating the unchanged-production baseline as R2;
no assertion changed. The repaired runs use those same R2 fixture bytes.

Debug and Release each pass 173 tests with zero failures and one existing
ignored DMA microbenchmark: 154 core NVMe tests plus 19 existing PCI-platform
NVMe tests. All 17 new cases and all 14 prior Identify regressions pass.
These broader counts are distinct from the seventeen-test baseline filter.
Raw SHA-256 values are:

- R1 baseline: `8bf74f199743bfdf6fcf6f63b8fe2be92b783a696c225dfc2fd2fa166d17ef1d`.
- R2 baseline: `12f623826b5853f366a81deb08c20be15dab3533cfac5d5dea6e299f0f47fe23`.
- Debug: `42f960a19a9a6c13260b454d74497f9ea3ce20084008a3b077b9fc2c0dbb368a`.
- Release: `fb300976632daa420deb17035095e483a8c5620a8898f20a1a8a5f2db5159ae6`.

Focused Clippy, formatting, budgets and whitespace checks pass. Each fixed
check binds all 49 NVMe/budget files and the staged tree before and after.
Root independently rehashed 75 unique pins and cleared focused integration;
a separate review returned CLEAR_NARROW. Review SHA-256 values are
`3ca40b3ddcdb841de13e21318cc4148301b8bfcbbd51b2c74a4f63ae05456fe5` and
`02de23a5e448dc184e9773439b3fd57e4ab8a8585b111f53ed6dbec606b7f7e3`.
The APST exclusion was independently confirmed from the primary specification.

Existing ceilings fall from 84 to 77 for admin dispatch, 81 to 77 for log
handling and 66 to 50 for Identify handling. New ceilings are 27 for the
shared helper and 168/153 for the two fixture files, all with unsafe 0.
Integration preserves all eight reviewed nonbudget files; the sole budget
append conflict is resolved as an exact union with the four parent doc rows.

## Integration limits

Combined full-project and exact-SHA hosted validation of this change remain
pending. Earlier failed and incomplete integration evidence stays in the
[Identify history](native-vmm-nvme-identify-prp-20261005.md), including the
original W16 local 37 PASS / 8 FAIL and hosted acquisition failures.
No VM, Windows workload or physical-storage behavior was measured. Product
state, capability wording, criterion thresholds and known defects are unchanged.
This scoped deterministic repair does not promote a live criterion or release.
