# Native VMM MSI delivery failure — 2026-10-05

Source `633c774125afd9d629070f0bd8a636bce27d66bf` follows the SPI and
own-board firmware checkpoint `fb9cd0830f41f591b68faa6cf6a322494bb5aec7`.
The new repair has deterministic evidence; no live provider failure is claimed.

## Failure and repair

The old MSI-X loop counted a nonzero provider return and continued sending the
batch. Its completion adapter then discarded failure and permitted the primary
or secondary run loop to continue. A real NVMe Identify fixture demonstrates
why this matters: DMA and the completion entry succeed, the pending MSI is
consumed, and the unchanged device state does not regenerate the message.

The loop now returns the first failed message and its raw provider status.
Only attempted deliveries are counted; already accepted messages are not
replayed and the unattempted tail is not sent. Completion records successful
SPI work once, clears the consumed scratch buffer and propagates the MSI error.
The existing four primary/secondary pre-run and post-MMIO stop boundaries
handle a common typed error. Secondary failure publication and CPU0 wake still
use the existing fatal lifecycle path, including the post-join reset veto.

Diagnostics retain vector, guest-programmable address/data, raw status, drain
location, exit and PC. A provider rejection may also result from malformed
guest MSI configuration; its raw status alone does not establish the cause.
The native backend stops on any nonzero result without retry. The existing
MSI entry in `docs/machine-contract/qemu-virt-deviations.json` documents that
guest-visible policy and links its detailed contract. Optional userspace GIC
delivery behavior remains unchanged.

## Deterministic verification

Eight paired MSI tests produced 1 PASS/7 FAIL against the extracted old loop
and completion policy, then 8 PASS after repair. The real NVMe fixture uses
the advertised MSI frame address and INTID 128, so it exercises propagation
without depending on malformed guest configuration. Final integrated focused
checks pass all 18 interrupt cases and 28 existing CPU lifecycle cases.
Clippy, workspace formatting, JSON parsing and documentation references pass.

The first structural-budget check failed because the deviation registry grew
from its 80-line ceiling to 89 lines. The detailed entry was extracted into a
9-line contract file; the root MSI entry still explicitly states the policy.
The final budget check passes without raising any existing ceiling. Thirteen
Rust files retain the exact bytes covered by independent source review; root
also reviewed the documentation extraction and authenticated the final handoff.

## Evidence limits

Preceding FB9 passed the mandatory local project check: 44 stages, zero failed,
910.024602 seconds. Its exact manual CI run 37262230042 passed all 14 jobs;
PR 310 passed 144 checks with one intentional skip on its separate merge
revision. FB9 hosted full run 37262227685 was still pending at this checkpoint.
The previous failed T12 firmware-pin attempt remains failed.

Separately, development-only A572 boot campaign
`6b60201b163e6fc2527ac82dc96013e7` remains failed/incomplete: the first lane
showed an expired-account prompt at Windows login and reached no desktop;
the other seven jobs were canceled. The eight-job requirement is unchanged,
and the existing reporter rejected the campaign with zero valid performance
samples. This is not a diagnosis of an interrupt or timer failure.

This MSI checkpoint still requires its complete local project check and hosted
checks at the exact pushed SHA. No Windows performance improvement, live
compatibility result, criterion pass or release promotion is claimed.
