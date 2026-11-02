# Live-input host preparation outside the platform lock (2026-11-02)

The live-input tick read and compacted its host control file while holding
VirtPlatform, and its exclusive flock could wait for a writer. Host file
preparation now runs after the prelude guard is released. Pending commands
are applied under a new short guard with a fresh Instant, so an intervening
HDA tick cannot make input injection rewind the platform's host clock.

The periodic reader attempts a nonwaiting exclusive lock. A busy writer
leaves the offset, partial frame and pending queue untouched; normal16ms
polling retries later, while already queued device work may still advance.
Startup length/watch access keeps its existing blocking semantics. Owner,
link-count, pinned-inode and replacement checks remain fail-closed.

The64-command queue, byte budgets, coalescing, compaction and restart replay
policy remain. Existing tick entry points are test-only adapters. Prelude,
panic/boot triggers, later agent work and stop decisions keep their order;
stop predicates were extracted under the existing structural ceilings.
Snapshot artifact output remains separate from input-file preparation.

The API-level focus passes38tests, including causal held-writer recovery
and actual input-TRB timing. After caller integration,45input/lock-site tests
pass and production clippy with warnings denied and rustfmt pass. An initial
macro trailing-comma compilation error is retained and repaired. The existing
acquisition-coverage test now covers all four actual Automation guards.

No live guest or audible responsiveness claim is made. Final exact project,
hosted and required PR gates remain necessary; criteria/state are unchanged.
