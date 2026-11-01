# Concurrent NVMe test scratch ownership (2026-11-01)

## Failure and reproduced mechanism (automated tests)

Exact `89b4283a` passed hosted full 41333196861 and all 103 associated
workflows, but its local workspace run failed with 1494 passed, one failed
and two ignored. `failed_namespace_sync_never_reports_shutdown_complete`
could not open its second scratch file at shutdown.rs:198 (ENOENT). The
original missing pathname was not retained, so attribution of that one
failure to a collision remains inferential. Hosted success does not erase it.

The existing test allocator used name, process ID and SystemTime nanoseconds.
Three concurrent shutdown tests share the same primary/secondary names and
unlink their files on drop. An exact-generator experiment with 32 threads
and 2000 barrier rounds produced 64000 names with 4618 duplicate calls; the
observed clock quantum was 1000 ns. Replaying one observed duplicate as
write A, write B, cleanup A, open B reproduces ENOENT. This establishes a
real fixture ownership defect independently of the original attribution.

## Repair and regression

The test-only allocator is extracted into scratch_path.rs. It retains PID
and timestamp and adds a process-wide atomic discriminator; checked overflow
refuses exhaustion instead of reusing a suffix. Production NVMe is unchanged.

A deterministic regression forces 32 callers to use four identical names
at one fixed clock tick. It creates 128 files with create_new, removes
even owners’ files and verifies every odd owner’s original bytes. Without
the discriminator it fails AlreadyExists; after repair it passes. Existing
shutdown workloads and assertions are unchanged. Structural ceilings fall
for the extracted helper and new files are registered at actual size.

## Separate execution-context observations

The original local run also failed an unchanged 1 ms median/20-sample frame
request assertion (median 16.194625 ms) and a native transport churn check
with its unchanged four-second timeout. Isolated same-binary frame-request
comparisons passed twice at standard nice 10 and twice under background
policy. Exact full-suite causes remain unproven; no latency or timeout limit
is relaxed. Successor exact local and hosted checks are required. These are
synthetic tests, not live guest or release-gate samples; A11 remains OPEN.
