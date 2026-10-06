# NVMe Identify PRP2 transfer — 2026-10-05

Integrated source checkpoint `d0b444e5f6f58eb2cfd62407a9595ae4dd6d7e8f`
repairs Identify's 4096-byte transfer when PRP1 has a page offset and PRP2
names a nonadjacent page. The old contiguous write could overwrite the page
following PRP1, leave PRP2 untouched and return success.

## Transfer boundary and architectural scope

[NVM Express 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
section 5.15.1, Figure 240 defines Identify's 4096-byte data transfer and
references Figure 105. Section 4.2, Figure 105 requires PRP2 for the second
page when that transfer starts at a nonzero PRP1 offset; section 4.3 defines
the pointer offset and alignment rules.

The extracted `identify_command.rs` handler uses the existing PRP decoder
and writes the selected payload into its ordered spans. It restores and
clears the reusable scratch vector on normal status-return paths. Existing
PRP1 alignment policy, payload builders, CNS selection and invalid CNS/CSI
statuses remain unchanged. An invalid later destination can leave an earlier
intended span written; this does not provide atomic DMA rollback.

Reversing the handler extraction and transfer change recovers the original
builders and dispatch byte for byte. No existing test assertion changes.
The `identify.rs` ceiling falls from 202 to 147 lines. New ceilings are 66
for the handler, 131 for the fixture and 99 for support, all with unsafe 0.
No existing ceiling increases.

## Paired fixtures and retained failures

Fourteen unchanged corrected fixtures yield baseline 2 PASS / 12 FAIL. Nine
scattered-transfer methods cover controller, allocated and unallocated
namespaces, active namespace lists, namespace descriptors and command-set
controller data. Each tries offsets 4, 512 and 4092 with nonadjacent PRP2.
Baseline methods stop at offset 4's first failed assertion; the later offsets
are covered by the passing repaired runs, not separate baseline failures.

Six sentinel-filled pages expose writes outside the intended spans. A public
aligned Identify reference and independent payload anchors check contents;
CQE checks cover status, CID, SQ head, SQ ID and phase. Controls cover unused
PRP2 on aligned PRP1, missing or misaligned PRP2, and unbacked first or second
destinations. Fixture SHA-256 values are:

- Tests: `7fbda5656cf87eceb76b416eca25a2ffc66ac283507ddce572eeae328750cd9a`.
- Support: `8e37d76b5d85071ab01fb04c02d67f54150de04a3f1f07a2b9f36037828901a1`.

The first baseline reported 1 PASS / 13 FAIL with a mistaken descriptor
anchor: it expected EUI64 first, while the unchanged builder puts UUID first.
Two descriptor cases and the aligned control failed on that fixture error
and are not PRP regression evidence. Only the anchor was corrected before
rerunning the unchanged production baseline. The R1 record remains retained;
its support file was reconstructed by reversing that correction and matched
the historical hash, rather than being separately retained at the time.

All fourteen corrected cases pass in both repaired profiles. The broader
`cargo test -p bridgevm-hvf --lib nvme` filter passes 156 tests in debug and
release: 137 core NVMe tests plus 19 existing PCI-platform NVMe tests, with
zero failures and one existing ignored DMA microbenchmark. These broader
counts are not the fourteen-test baseline filter. Retained raw SHA-256 values:

- Initial baseline: `bdae04d740e40e64d527c1d9c8c5d5458c0d0651889ede04cc7910123ecac149`.
- Corrected baseline: `ea71ddf9d78130baef4161eb42e01b26b8dd4fbfffe23dd40d4ab944d15e4718`.
- Debug: `527dc5925efcde7e230d509e4eca2dffdbcf377820b3a4a2b1a83d30324c83fa`.
- Release: `ff35444a74a31140fc4a0f078e86a4c96473cf792028dea88daa94c3edb27061`.

Focused Clippy, formatting, budgets, whitespace and source/evidence review
pass. Integration retained all reviewed nonbudget source bytes; its later
budget-append conflict with the shutdown repair was resolved as an exact
registration union without raising any prior ceiling.

## Limits and incomplete integration evidence

Other admin transfers and controller-reset policy are outside this repair.
No VM, Windows workload or physical-storage behavior was measured. The W16
local project check remains FAILED at 37 PASS / 8 FAIL. Two hosted jobs failed
during runner acquisition. By 2026-10-05 22:18 UTC, hosted full had 44 PASS and
manual CI 14 PASS, but other failures keep validation INCOMPLETE. Successor checks remain
pending. Canonical capability fields, criterion thresholds, known defects and
product wording are unchanged; no live criterion or release promotion follows.
