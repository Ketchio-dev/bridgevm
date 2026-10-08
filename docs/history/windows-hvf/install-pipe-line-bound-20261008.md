# Bound installer pipe line retention — 2026-10-08

Evidence: deterministic synthetic native Swift tests and static review. No
Windows installation, live responsiveness, RSS or callback-backpressure proof.

LineAccumulator, used by the Windows installer pipe readability callback,
appended every delivery before looking for a newline. Sixteen newline-free
1 MiB deliveries actually retained16 MiB. A separate oversized-line completion
also emitted the entire oversized record with its protocol-looking suffix.
Root baseline two tests/three failures is retained. This is distinct from the
already-bounded TailOffsetReader path, not a duplicate repair of that parser.

The extracted accumulator scans incoming fragments, retains at most2 MiB per
logical incomplete line, and discards overflow through its next newline. Its
suffix never becomes a separate record. Exactly2 MiB is accepted; LF is not
counted, CR and UTF-8 bytes are. Empty input does not flush or reset. NSLock
still serializes complete append operations. Valid nonempty UTF-8-only output,
CR preservation and blank/invalid-line dropping remain unchanged, unlike the
file-tail decoder's intentionally different behavior.

The bound applies to pending logical bytes, not total input allocations, batch
output, queued main-actor callbacks or process memory. No process lifecycle, EOF
draining, tail parsing or log-delivery backpressure change is included.

Root focused16/0 covers every1 MiB append, overflow/suffix recovery, exact
boundaries, large delivery of short lines, split UTF-8, invalid bytes, valid
replacement character, CR accounting and concurrent complete records. Removing
the fragment limit reproduces retention/output failures (two tests/30assertions);
removed, restored broader Windows-install/line selection156/0. Existing native
framework duplicate-class warnings remain in logs; no warning suppression.

First sealed full failed: the shim lacks XCTAssertLessThanOrEqual, though
native156 passed. Same <= predicate uses supported XCTAssertTrue; no relaxation.
Failure retained; new exact full/hosted proof required. Budget extraction and
product/worker/fence/permissions/media unchanged; no release/guest claim.
