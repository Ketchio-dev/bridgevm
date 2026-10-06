# Native VMM unexpected secondary stops — 2026-10-05

Source `871c5457` follows the MSI failure checkpoint
`97b66c8ca5485979afc9e38e32f208d86251c8a8`. This repair has deterministic
regression evidence; no guest-stall cause or live improvement is inferred.

## Reproduced failure and repair

Four secondary run-loop leaves printed a diagnostic and returned without
publishing a fatal error: an unexpected exit reason, unsupported system
register, unhandled exception class and exhausted exit cap. The owner then
published its final snapshot and withdrew its vCPU handle while retaining
PSCI state On. Primary continuation and joined reset decisions checked the
error flag, so they accepted that unexpected owner-thread exit as healthy.

A shared helper now records the existing fatal flag before waking CPU0 and
returning from each of those four leaves. The primary pre/post-run guards
and joined reset veto therefore receive the failure through their existing
lifecycle path. Diagnostics retain the actual reason or trap, ESR and PC.
Normal CPU_OFF, global shutdown and PSCI terminal paths are unchanged; the
repair does not fabricate guest CPU_OFF from an unexpected host-side stop.
Constructor panic behavior remains outside this change.

## Paired deterministic evidence

The regression uses the actual VcpuControl publication and withdrawal,
an owner thread, primary entry guard, shutdown/join and both reset policies.
Mechanically restoring the old four leaves produced 1 PASS/4 FAIL: the owner
was absent with state On, zero wakes, allowed primary entry and allowed reset.
The repaired cases pass 5/5; final lifecycle and interrupt suites pass 33 and
18 cases. Focused Clippy, formatting and structural budgets also pass.
The snapshot and failed wake status are synthetic, not HVF observations.

Root read the source and evidence, then verified all six integrated Rust
files match the reviewed author bytes. The sole cherry-pick conflict was
append-only budget registration; both sets of rows were retained, with no
existing ceiling raised. An unchanged error-formatting implementation was
extracted to keep the lifecycle module within its previous ceiling.

## Dated validation boundary

Preceding exact 97 passed the mandatory local project check: 44 distinct
stages, zero failures, 911.773 seconds; raw log SHA-256
`df2ce36c26a7415fb62de670bbab1711cb3472abe720cd120fcc00c197aa38ce`.
Its exact hosted full run 37265178124, manual CI 37265180122 and separate
PR 311 checks remain pending at this source checkpoint.

Earlier exact FB9 completed its local check, exact hosted full and CI, then
passed the separate fixed-N=20 physical T12 gate. See the
[T12 record](native-vmm-t12-nvme-20261005.md) for its raw-log audit and limits.
The earlier failed C23 firmware attempt and failed/incomplete development
Windows login campaign remain failed; neither is reinterpreted here.

This new source still requires its mandatory local project check and exact
pushed-SHA hosted checks. All product criteria, thresholds, known defects and
product state remain unchanged. No release head or Windows improvement is
claimed.
