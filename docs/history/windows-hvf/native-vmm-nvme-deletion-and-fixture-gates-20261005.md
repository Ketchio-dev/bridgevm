# NVMe queue deletion and fixture gate repairs — 2026-10-05

The own-VMM NVMe controller previously reported successful queue deletion
without removing the queue. With a depth-two completion queue already holding
one result, a second write waits for completion space. Deleting its submission
queue returned success, but consuming the old completion let that queued write
modify the disk after deletion had completed.

## Deletion repair and deterministic evidence

Author21f753d3 integrates as9ee9d1d7. Deletion now removes the SQ and pending
work before the admin completion. A deleted SQ cannot be replayed by the drain
loop's earlier pending-word snapshot because it rechecks current queue state.
CQ deletion rejects any still-attached SQ and preserves traffic on refusal.
Admin, absent and already-deleted queue IDs return command-specific errors.
Creation, reset, snapshot format and synchronous execution remain unchanged.

This implements the existing advertised deletion behavior in
[NVMe1.4 sections5.5–5.6](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf).
It introduces no intentional guest-contract deviation and does not establish
general NVMe conformance. The platform remains a QEMU virt-compatible contract
with documented deviations.

R1 reproduces one failure with a passing reset control. R2 expands to five
cases but its recreation CQ address was not page-aligned; that experiment is
retained. R3 changes only the synthetic memory capacity and recreated CQ page.
The same corrected R3 test bytes produce1PASS/4FAIL on original production:
raw SHA `1b25a6b883f0f6f812f63d8103467ad3814edad4742ecf1aea72480bdd991c3d`.
Repaired debug and release each pass94 cases with one pre-existing ignored
throughput benchmark. Coverage includes stopped writes/completions after
deletion, attached-CQ refusal, invalid IDs, new-address recreation and reset.
Formatting, Clippy, budgets and the paired-byte comparison pass. Independent
review authenticates38 inputs; root separately verifies24 source/evidence
pins. No real disk, Windows guest or physical hardware was used.

## Preserved integration failures

Exactfae230d0 full check failed after832.948 seconds with41PASS/3FAIL:
documentation, shell lint and product/A19 contracts. Raw SHA
`a81b98c9ccc3a80abfe431e8a99f4eb3ef76a9164feaff8f25a8b76d33f25aa2`.
Exact6d5c6133 then failed after862.103 seconds with42PASS/2FAIL: documentation
and product/A19 contracts, with the explicit-argument shell repair passing.
Its raw SHA is
`c81e43b34508da17989256f67ad96694311968a05a49b1bdc97e4017106d211d`.
Both runs kept exact clean source; neither is admitted as live evidence.

Root introduced nested documentation-manifest includes, which the manifest
reader forbids. Source669263ea replaces them with one supported include level,
preserving all four current/history rows. The documentation check then passes
309 classified documents and531 link targets. Existing ceilings are not raised.

Exactfae T20 workflow37326111361/job111817161639 also failed an obsolete T23
literal-wrapper assertion after the verifier delegated non-T1 tiers to legacy.
That tests-only correction and its downstream contracts are under review at
this checkpoint; no passing result is inferred from a partial suite.

## Verification boundary

The combined successor requires a complete project check and exact pushed-SHA
hosted checks. Earlier F59, fae and6d failures remain in the record. No physical
storage result, guest performance gain, criterion pass or release promotion is
claimed by these deterministic repairs.
