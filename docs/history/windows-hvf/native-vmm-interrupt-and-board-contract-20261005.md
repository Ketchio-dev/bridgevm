# Native VMM interrupt delivery and own-board firmware — 2026-10-05

This checkpoint follows source `a572547b6f6c90214358d783091ce1caaf598738`.
Evidence here is deterministic except the explicitly failed physical-queue
attempt below. Product truth remains in `capabilities/windows-hvf.json`.

## Failed SPI delivery stops the VM

The runtime previously drained pending interrupt-level changes, counted a
nonzero provider result and continued execution. A device with an unchanged
interrupt level does not necessarily queue that transition again, so continuing
could lose the notification. The repair returns the actual interrupt ID, level,
provider status and run-loop context on the first failure. It counts only the
requests actually attempted and never replays an already accepted request.

All four CPU0/secondary pre-run and post-MMIO sites now reject continuation on
that error. Platform mutexes are released before stop handling. CPU0 records a
fatal reason; a secondary publishes its existing fatal flag and requests CPU0
exit. The previously checked post-join policy suppresses any pending reset on
fatal failure. Successful delivery retains the existing completion path.

An extracted old provider loop with the new caller seam produced 2 PASS/8 FAIL;
the repaired suite passes all 10 cases. One case uses a real VirtPlatform block
completion to show that an unchanged level does not regenerate a drained SPI.
The existing 28 secondary lifecycle tests still pass. Formatting, Clippy,
structural budgets and independent review pass. No physical Hypervisor API
failure was captured, and this is not a diagnosis of an earlier guest stall.
MSI-X failure handling is outside this change.

## Own-board firmware uses the modeled inventory

T12 job `t12-native-c23d20ec-20261005-r1` at
`c23d20ec6f47c071515ee7f1f9e95b05840486e3` failed before VM execution because
the builder and runner required different firmware digests. It attempted one
lane, recorded zero passes and performed zero firmware boots or Block I/O reads.
The built FD matched the builder's existing component and source pins. History
identified `180e56b6e76e9f82706e4158be910b38ffdd29ff` as the change that had
updated those pins while retaining an older runner digest. The failure remains.

The builder, compiled validator and opt-in smoke now read one strictly parsed
approved FD digest. Unknown firmware, wrong size and malformed pins remain
errors. The new digest derives from the source repair below, not acceptance of
unexplained output bytes. Other component pins remain enforced.

A separate producer/consumer mismatch required eight PCI functions although
BridgeVmPcPlatform currently models only the host bridge, NVMe and xHCI. The
actual C producer and Rust consumer now require those exact three identities.
The eight-slot, 128-byte result storage ABI is unchanged; the remaining five
slots must be zero, and the producer clears them explicitly. All root bridge,
driver binding, BAR and positive NVMe Block I/O fields remain required. The
wrong-count path also releases handles obtained from a successful enumeration.

An actual-C baseline with ownership cases produced 2 PASS/14 FAIL; the fixed
producer passes all 16 cases. The Rust baseline produced 15 PASS/2 FAIL; the
fixed consumer passes 20 tests. Strict pin tests pass three cases with multiple
malformed inputs and drift mutations. These checks are reached by the existing
firmware boundary and hosted/project example-test stages.

Two complete pin-enforced firmware builds, one from a separate clean EDK2 tree,
produce the same FD digest
`6b041c345f6707d38b388d232b0a9ce63a5ba6153d31462f9071da9943571aa8`.
Both pass the actual pure firmware validator, which rejects a one-byte mutation
of each. Intermediate strict pin failures and fixture/formatting mistakes remain
recorded as failures. Independent review authenticates the source, build records
and artifacts; it does not turn deterministic evidence into a guest result.

## Verification boundary

The preceding A572 source passed all 44 local project stages and all 14 jobs in
exact-source hosted CI run 37259149052, including cross-compilation. Hosted full
run 37259147110 passed 43 stages and conditionally skipped cross-compilation.
PR 309 passed 13 required jobs on its separately identified merge revision.
The earlier C23 full/CI outcomes and failed T12 remain in the preceding history.

This checkpoint still requires its full local check, exact pushed-SHA hosted
checks and a fresh physical T12 campaign. T12 remains 20 independent lanes and
40 successful firmware Block I/O reads. No guest symptom, performance gain,
Windows support on the own board, criterion pass or release promotion is claimed.
The existing QEMU virt-compatible Windows platform contract is unchanged.
