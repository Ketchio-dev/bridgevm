# NVMe WRITE cache and FUA completion — 2026-10-05

Integrated source checkpoint `68935219af9f6a6a69750c3f3aaba9330c2e60dd`
repairs subsequent WRITE completion after disabling the volatile write cache,
and WRITE commands with Force Unit Access set. Previously both returned the
data-transfer status without invoking the existing backend synchronization.

## Completion boundary and architectural scope

[NVM Express 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
section 5.21.1.6, Figure 281 defines cache enable. Section 6.4.2.1's informative
example describes completed-write preservation and its enabled-cache,
FUA-clear exception. Section 6.15, Figure 402 defines WRITE CDW12 bit 30:
FUA data must reach nonvolatile media before command completion.

After a successful direct or buffered WRITE, disabled cache or FUA invokes
the existing selected-namespace `io_flush` before the queue posts completion.
A raw write-back file reaches `sync_data`; its failure returns the existing
internal-device-error status. Data already written is not rolled back.
Failed data transfers keep their original status and do not gain a flush.
Ordinary cached writes retain their existing behavior.

`io_write.rs` extracts the three WRITE methods. Removing the five-line sync
hook, restoring the original comments and restoring the split-boundary blank
line recovers the original `io.rs` byte for byte. No existing NVMe test body
changes. The `io.rs` ceiling falls from 279/unsafe 2 to 168/unsafe 1; the new
WRITE module is 120/unsafe 1. Three fixture files have actual ceilings
174, 88 and 32; no existing ceiling increases.

## Paired fixtures and retained failures

Nineteen unchanged new test methods give baseline 2 PASS / 17 FAIL. Eight
disabled-cache cases, eight FUA cases and restored-cache state fail separately;
the ordinary-cache and rejected-transfer controls pass. Cases cover both
direct and buffered DMA, both namespace IDs, sync success/failure, reenable,
snapshot restore, invalid LBA, unbacked PRP and a real host write failure.
They inspect actual CQE status, CID and phase, both namespace sync counters
and both backing-file contents. Fixture SHA-256 values are:

- Tests: `17c3f28ed19066cad4c6b9f0e443137c404abbc7d8ea2087cc82c3969810bb01`.
- Support: `8ca6779e9b9c811fdd87c34d4b8cc0ef1d3612e5f7f7428b791c8a811adfc232`.
- File ownership: `df0010f8a75cdda0395fba5cab65b88d8a170cff006c25568edf6d2638bbb18d`.

The first baseline aborted when fixture cleanup found a missing file while
unwinding; it has no completed suite count. A filename collision was only a
hypothesis, not an established cause. The second fixture revision uses an
exclusively created directory and process-local sequence. Those revised
fixtures were frozen before the completed baseline and used unchanged after
the production repair. The original aborted run remains recorded.

The whole NVMe filter passes 123 tests with zero failures and one existing
ignored microbenchmark in debug and release. Initial debug inventory omitted
the newly extracted production file, so that receipt does not independently
bind it. An unchanged-source debug successor again passes 123/0/1 and pins
all eight changed files plus the exact staged tree before and after execution.
The earlier receipt is preserved. Retained raw SHA-256 values are:

- Aborted baseline: `66aafd4ce7204343f41cdd6f2724f2cc45460b169fa8b691627ca0be261d3de2`.
- Completed baseline: `c3e20b231dffeaf91a03d494fb080d3a2b01d6e3c9a6818ca98fa7c2c11c62d8`.
- Initial debug: `baa7b832223a7fd7e7da14601216eb96669a7a27f865d14c066d4ad47dd8cb7c`.
- Bound debug successor: `ab5d42e3d03c31b7213b7d665d6480f11aaaa225e63d3134ca1a345eb6e48b46`.
- Release: `257eda41d123791ded2f456ae780cef95c1cfeda82f86ba2e34483ce537d4757`.

An initial inverse-extraction reader omitted the split-boundary blank line;
its shell continued into release compilation after that reader failed. The
corrected reader proved exact extraction without changing source. Focused
Clippy, formatting, budgets, whitespace and independent source/evidence review
pass. Integration's budget-append conflict was resolved as an exact row union
without increasing any existing ceiling; all seven nonbudget files match the
reviewed source.

## Limits and incomplete integration evidence

This establishes the existing backend sync/error boundary, not physical
power-loss durability. Memory and read-only overlay volatility are unchanged.
READ FUA, cache reset/default policy and failed SET FEATURES state policy are
outside this WRITE repair. No VM or Windows workload was measured.

The parent local project check remains FAILED at 37 PASS / 8 FAIL. The parent
hosted campaign remains FAILED/INCOMPLETE, with verified runner-acquisition
failure and no actual full-project result at this checkpoint. Successor full
project and exact-SHA hosted checks remain required. Criterion states,
thresholds, known defects and product wording are unchanged; no live criterion
or release promotion is claimed.
