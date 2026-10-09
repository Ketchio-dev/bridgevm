# A19 archive and interrupted-operation boundaries — 2026-10-09

Evidence rank: deterministic synthetic tests and source review. No Windows
workload, physical power-loss, final release campaign or criterion pass.

## Unreadable public hints

Actual CLI tests reproduced ten accepting subcases across T21/T22 after job
and ledger tiers were changed to a generic tier: oversized input, a tier beyond
64 KiB, a symlink, truncated JSON and invalid UTF-8. A writerless FIFO blocked
until the test's ten-second timeout. Deep nesting already refused. The hint
reader now propagates bounded-read and parse failures before any legacy output.
Valid generic JSON retains byte-for-byte output through the 65,536-byte limit.
The generic publisher has no universal output-size ceiling; this intentionally
refuses oversized or malformed historical data rather than inferring its tier.

## Prepared-input ownership

The T22 runner deleted a pre-existing synthetic `prepared-inputs/sentinel` after
preparation raised FileExistsError, then claimed verified cleanup. Preparation
now has an optional keyword-only allocation callback immediately after mkdir.
T22 captures the directory identity before cloning and reuses owned-retention
cleanup. Existing or replaced names are preserved; partial preparation can be
cleaned, but uncertain children and live paths retain media. Failed quarantine
cleanup remains unverified even when the original pathname is absent. Other
callers retain the optional callback's default behavior.

## Auxiliary source correspondence

The original helper orchestration accepted a seed with the correct disk and
wrong same-size vars when its manifest and selected hashes agreed. Its collector
also accepted six coordinated wrong-source substitutions: disk or vars in swap,
create, or both auxiliary cases. The seed now matches the captured phase-three
source sizes and hashes before restore. Production collection binds both members
of each old/source pair to first-case preinterrupt hashes transactionally.
Later guest boots use separate exports; final boot-mutated hashes are not this
source. The synthetic first-case fixture now reflects that timing. Historical
receipt validators, sample counts and passing thresholds are unchanged.

## Retained checks and limitations

Combined working-tree full check: 45 stages passed, exit zero, 2026-10-09
03:45:37–03:59:05 UTC; log SHA-256
`ef0ddeb115117688803638af42f244859f4b058de75aca74d1a12bcf58916d8b`.
The tracked diff and all six new-file hashes were unchanged across that run.
Fresh focused suites passed 15 archive, 11 cleanup, 14 collection and 15
auxiliary tests. Existing tests remain reachable through extracted mixins.
No ceiling was raised; new modules are registered at their actual sizes.

Failed experiments remain retained: the initial aggregate hit a 90-second tool
timeout; an inline fixture import lacked its search path; an isolated baseline
namespace lacked `__file__`; a mistyped unittest selector ran zero tests. Corrected
in-memory baseline runs reproduced one seed and six collector failures, then
restored focused tests passed. No temporary instrumentation remains.
Exact sealed-commit local and GitHub-hosted checks are still required at this
record's creation. A9, A11 and A19 remain OPEN; no product promotion is claimed.
