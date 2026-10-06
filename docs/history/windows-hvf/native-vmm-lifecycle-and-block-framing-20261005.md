# Native VMM lifecycle and block framing — 2026-10-05

This checkpoint extends the independently checked native engine source
`c23d20ec6f47c071515ee7f1f9e95b05840486e3`. Evidence below is deterministic;
capability states and wording remain in `capabilities/windows-hvf.json`.

## Secondary CPU failure reaches the VM owner

A secondary CPU previously set its error flag and exited, while CPU0 inspected
that flag only during final shutdown. CPU0 could therefore keep running after
losing a secondary CPU. The owner now publishes the error before requesting
CPU0 exit; CPU0 checks the flags both before entry and after a successful
Hypervisor return. Typed errors preserve actual Hypervisor return values and
identify secondary failure without inventing a Hypervisor status.

Apple's Hypervisor header documents that an exit request also makes the next
run return immediately if the vCPU is not running yet. A failed exit request
is retained as a diagnostic; immediate wake is not guaranteed in that case.
The error flag persists for the next guard. Primary handles remain valid until
secondary joins finish, and normal owner cleanup still controls destruction.

Initial review found a further gap: a failure arriving after CPU0's last check
could be collected during join while an already requested reboot/recreation
still proceeded. The post-join merge now cancels either reset action on fatal
failure, preserves an existing primary-fatal reason, and otherwise records a
truthful secondary-fatal stop reason.

An extracted old entry policy produced 3 PASS/4 FAIL; an extracted old late-join
merge produced 2 PASS/4 FAIL. The final coordination suite passes 28 tests and
three existing reset-policy tests pass. Focused Clippy and independent review
pass. The initial incomplete repair and formatter/budget feedback remain in
the development record. Integration preserves all eight reviewed source files.
Integrated source: `67d677e7f701078a8f42bbecccbe59f6df666bc8`.
Late-join baseline SHA-256: `a487a5598b17a795275bbeb870532d2cf71e13106e134b41f9668e362f55d5d2`.
Final coordination log SHA-256: `17212dd35c245d3d7185bfeadd4e31937ccead82cecb3326ddd335271dbfb7c5`.

## VirtIO block requests use logical buffers

The read-only block device assumed a complete header in the first descriptor,
separate data descriptors, and a standalone status descriptor. It now assembles
the header only from offered readable bytes and treats the writable buffers as
one logical data-plus-status stream. Split headers, scattered data, a combined
data/status descriptor and empty trailing spans work without new request-time
buffer allocations. Direction ordering, inclusive address ranges, sector-sized
data and media bounds are checked before data copying.

Completion length describes the initialized writable prefix. A failed status
write is not counted, and status after an unwritten data gap does not extend
the prefix. Read-only OUT returns IOERR; unknown commands remain unsupported.
OUT diagnostics retain the readable payload length. These follow
[VirtIO 1.2 sections 2.7.4.1, 2.7.8.2 and 5.2.6](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html).

Six regressions failed against unchanged production code. The first passing
24-test revision still missed the prefix requirement; it was corrected after
reading the specification. Additional partial-copy, zero-length, zero-address,
range and termination cases passed. An independent OUT-status regression also
failed before its repair. A subsequent parallel test run had 31 PASS/1 FAIL
because fixture filenames collided; unique per-process fixture sequences fix
that test issue. An intermediate test-method typo failed compilation and was
corrected. None of these failed experiments is counted as a successful check.

The final focused suite passes 32 tests. Whole-tree formatting, structural
budgets, library/test Clippy and independent source review pass. Existing
oversized-request and PCI read-only tests now assert the stricter correct
completion length and status. No existing structural ceiling increased.
Integrated source: `19a821e67d24eaf29338b3a15b80a0f66ebb552b`.
Six-case baseline SHA-256: `43129157839a8761d3f3c8dd4a70a6560fcb1c738f9974ad7f3af9f9ddf64cfd`.

## Verification boundary

The preceding C23 source passed all 44 local project stages. Hosted full run
37255744729 passed 43 stages and conditionally skipped Linux cross-compilation;
exact-source CI run 37255746180 passed all 14 jobs, including cross-compilation.
PR 308 settled at 144 SUCCESS and one intentional SKIPPED. Its earlier failed
FE checks remain recorded. T12 job t12-native-c23d20ec-20261005-r1 failed
before guest execution because the built FD and runner digest pin disagreed.

Full local and exact pushed-SHA hosted checks for this second checkpoint are
pending. No guest symptom, performance improvement, release-head designation
or criterion pass is inferred. The changes correct existing contracts and add
no intentional platform deviation. Product state and known defects remain.
