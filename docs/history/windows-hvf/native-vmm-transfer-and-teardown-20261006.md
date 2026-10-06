# Native VMM transfer and teardown boundaries — 2026-10-06

Four focused repairs address HDA descriptor-address overflow, GIC input history,
secondary-vCPU joining and NVMe PRP-list length. Evidence is deterministic testing
and source review, with no live or release claim. Integrated source: `e0689d39ad0d4f1a14adbaf1b54d796a732571a9`.

## HDA stopped-stream descriptor rebasing

After eight valid 192-byte descriptors, guest MMIO stops RUN, writes the maximum
128-byte-aligned BDL base and resumes. Saved index eight makes the next descriptor
address overflow. The earlier rejected BDL-rewrite hypothesis predates the later
RUN-resume repair that preserves the cursor; that historical record remains intact.

Checked addition follows existing failure handling: DESE, clear RUN and unchanged
PCM, LPIB, position-buffer data and cursor. Unmapped and highest non-overflowing
address controls reach the memory provider; the overflowing case does not.

R2 old Debug/Release each gave **2 PASS / 1 FAIL**, with real overflow panics;
initial fixed core HDA runs each passed **29 / 0**. The first fix was committed
before observing Clippy's terminal, which returned 101 for `identity_op` in two
test constants. That sequence error and failed check are retained. The equivalent
`!0x7fu64` cleanup followed in `1dd21f6cdcfc84eb9018dfa6e87c22f8c44148f1`.

Definitive Rust 1.97.0 R3 pairs use identical corrected fixtures: original
Debug/Release each **2 PASS / 1 FAIL** (overflow), fixed each **29 / 0**;
corrected Clippy passes. Fixture SHA-256:
`d449346fd55953635712f2b6377cce00935260fd627b78e1769b49d18aecd23e`.
These are core HDA tests, with no audio-quality or live playback claim.

## Userspace-GIC electrical input history

The model discarded its remembered high input after latching an edge. A
repeated high sample after acknowledgement or pending clear could therefore
create another edge. A separate electrical-input bitmap preserves that history
while existing pending/level visibility and MSI message handling stay intact.

Eight unchanged fixtures give **59 PASS / 4 FAIL** with original production
(55 existing tests and four new controls pass), then **63 / 0** in both Debug
and Release. Cases cover held-high repetition during/after service, explicit
pending clear, a real second edge, falling input, level input, MSI and cloning.

This repairs the rising-edge contract in [Arm IHI 0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269), sections 1.2.1 and 4.1.2.
It proves model-API behavior only: current platform producers already enqueue
only changed levels. No ordinary guest/workload occurrence is established;
the default Apple GIC backend is unchanged.

## Join every secondary owner before propagating panic

The previous join loop could propagate the first owner's panic while later
owners remained unjoined. The extracted helper first collects every join
result, then applies the existing failure policy, diagnostic and healthy-result assembly.

Four unchanged regressions give **1 PASS / 3 FAIL** on the original behavior.
The repaired probe test target passes **489 / 0** in Debug and Release. The
first baseline command selected zero tests because it used a filename filter;
that run is retained and is not passing evidence. The corrected filter ran
all four intended fixtures. Fixture SHA-256:
`a5c25098e0a1292fd91b847fe8019e21d423aefbdbb923f5f39c5c67db99d7cc`.

The new fixtures exercise production joining with controlled owner threads;
they do not execute complete secondary-vCPU loops or HVF provider operations.
The 300 ms early-return observation can miss a badly scheduled failure; it is
bounded regression evidence, not exhaustive scheduling proof.
No guest stall or memory corruption is attributed to this defect.
Earlier detection of a running owner's panic and watchdog cancellation remain outside scope.

## NVMe transfer-derived PRP-list bound

The decoder's fixed 16-list-page cap rejected valid transfers that need 17
pages, or 18 with permitted first-data/list offsets. The controller advertises
MDTS=0; the tested maximum NLB transfer is 65,536 logical blocks. A bound based
on remaining data pages plus one allows the first list to contain only a chain
pointer; later aligned lists necessarily consume data, keeping traversal bounded.

[NVMe 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
§4.3 (pp. 69–70) defines packed PRP lists and continuation alignment;
§5.15.2.2 (p. 173) defines MDTS=0. READ/WRITE NLB is zero-based 16-bit
(§6.9 Fig. 372 and §6.15 Fig. 402), giving 32 MiB with 512-byte logical blocks.

Twenty unchanged valid-setup R3 fixtures give **8 PASS / 12 FAIL** on original
production. The final whole-NVMe runs pass **193 / 0**, with one existing
ignored benchmark, in both Debug and Release using Rust 1.97.0. They cover
READ/WRITE, buffered/direct DMA, 16/17/18-list layouts, exact payload/guards and
late malformed-list rejection that preserves payload and the caller's prefix.
No rollback guarantee for a later DMA failure is introduced.

R1's uncompiled draft called a nonexistent decoder constructor. R2 reused
invalid legacy queue setup; both were corrected and retained, and R3 programs
entry sizes before enable with physically contiguous queues. Its old-production
failure was rerun. A later explanation-only move restored the existing 152-line
budget; no ceiling increase or unexecuted budget failure is claimed. Final
Debug/Release, Clippy, formatting, budgets and diff checks pass on that source.

## Combined validation boundary

Existing structural ceilings are not raised. Focused successes do not establish
combined project success: integrated full-project and exact-SHA hosted checks
are **PENDING at this checkpoint**. Prior failed experiments remain failed;
no capability state, criterion threshold, known defect or product wording is
promoted. No live hardware, Windows workload or physical-storage result is added.
