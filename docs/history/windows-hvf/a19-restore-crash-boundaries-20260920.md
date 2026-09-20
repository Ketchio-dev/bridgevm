# A19 managed-pair restore crash boundaries — 2026-09-20

Classification: historical deterministic evidence. Current product state and
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A11 and A19 stay OPEN.

## Exercised boundary

Source `12ed03ce4e10161b8c9bf974fee735b41c74418d` adds a subprocess
test for the two sides of the managed restore's atomic directory publication.
The child acquires the real disk-plus-vars lease, stages a verified snapshot,
and then terminates through `std::process::exit` without Rust unwinding:

1. immediately before the atomic publication call; and
2. immediately after publication, before inode ownership refresh and removal
   of the initial-generation marker.

A fresh parent-side owner then reacquires the logical pair. The first boundary
must expose the complete old disk and old vars. The second must expose the
complete restored disk and restored vars. Both cases also perform another
restore so staged debris and an unacknowledged publication cannot make the pair
unusable on the next operation.

The helper is compiled only as a test module. Production restore behavior has
no fault-injection environment switch.

## Deterministic results

- the focused subprocess contract passed both fixed exit codes and pair checks;
- the complete `bridgevm-hvf` suite passed 992 tests with one existing
  intentional ignore;
- structural budgets passed with the new module fixed at its actual 109-line
  budget;
- the source-head full project check passed every executable, app, security,
  documentation and structural step and failed only the expected stale
  capability-registry identity before this checkpoint was recorded. Its
  7,647-line retained log SHA-256 is
  `789e908cd1c8227dec3e6ea2d4383294638af961192acf15b1ce7ff3ead839a8`.

## Evidence limit

This proves deterministic process-death behavior around the publication call. The first metadata head failed only because this record was unclassified.
Correction `b99e771d72c7669364f3766d7fce86291ad01cf9` fixed that, passed every other step, then correctly failed stale `tested_commit`; log SHA-256 is `ee4858707b7da36c73c4f47f1b6f13d53488db863b0aaceecd3f565d0c4d2b03`.
Final metadata head `01686593013ff8827bad78e7794e6bc12e7dc933` passed the complete project check; log SHA-256 is `bc8dfa35097b29400610960773c135584163c2c63eeeb05fa8aad7a7a8139532`.
It does not prove power loss, all fsync points, raw export or Windows boot. A19 remains OPEN.
Exact-head hosted verification remains required.
