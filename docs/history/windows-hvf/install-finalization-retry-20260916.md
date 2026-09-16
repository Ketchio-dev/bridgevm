# Installation finalization retry — 2026-09-16

Classification: historical deterministic evidence. The [registry](../../../capabilities/windows-hvf.json)
remains authoritative; A11 is OPEN and ENGINEERING_PREVIEW is unchanged.
This follows the [GUI startup worker record](native-gui-start-worker-20260916.md).

## Reproduced defect and implemented boundary

A failed finalization at prepared or diskStaged leaves a durable journal sealing
the original temporary disk and vars. Starting the same retained session again
previously reran validation, cache preparation, vars seeding and the installer.
Tiny synthetic files reproduced replacement before the old journal correctly
rejected the changed digest. No real Windows installation or guest media was used.

Source af5ea9abce49ab2391783af52d1c1d6857777fdf inspects recovery metadata before full source/ISO validation.
An existing valid journal resumes its authoritative durable publication path on
a retained detached worker; recovery cannot begin a new transaction or reinstall.
The ticket pins exact journal bytes and transaction identity, and the preserved
phase-appropriate request must match the accepted plan. Malformed, unsupported,
missing, busy or symlink state refuses without deleting the original inputs.
A missing transaction does not authorize reinstalling a saved completed VM.
Recovery opens only existing directory/lock descriptors; a concurrently completed
and removed transaction is never recreated by lock acquisition.

A private admission fence blocks callback reentry without making the existing
library admission reject itself. Accepted work remains reserved across awaits.
Fresh work reinspects inside the queued pipeline and after status publication,
before media effects. Cancellation before recovery starts leaves inputs intact;
once durable publication begins, the session retains the worker and reports its
actual commit or failure. Cancellation cleanup preserves journal-owned media.
The UI exposes recovery progress and disables cancellation during publication.
The existing finalization resume body was extracted unchanged for shared use.

## Deterministic record

- RED: combined control shim at the prior product source failed both new actual
  same-session regressions: 740 passed, 2 failed, 2 mandatory live-only skips in
  71.716 seconds. Validator/cache/prepare/process counts repeated and vars changed.
- GREEN attempt 1 retained: after the recovery cases ran, older queued-ownership
  fixtures hit the new saved-completed refusal; a fixture unconditionally popped
  an empty queue and trapped. Exit 133 in 66.500 seconds; this was not a pass.
- Those navigation/store fixtures now explicitly inject a fresh inspection beside
  their already injected validator; their ownership assertions are unchanged.
  Actual completed-state refusal remains tested through production inspection.
- Combined control GREEN attempt 2: 756 passed, zero failed, two mandatory
  live-only skips in 71.349 seconds, all recorded inputs unchanged.
- Final focused check: all 20 recovery/cancellation/boundary cases passed in
  14.378 seconds after rebuilding the product module, with stable inputs.
- Frozen full project check: PASS in 226.885 seconds at source af5ea9abce49ab2391783af52d1c1d6857777fdf;
  all 3,218 tracked inputs, HEAD and index unchanged. Shim suites passed
  425 / 760 (two required live-only skips) / 62 tests; native CLI/socket and release
  checks passed. Log SHA256 `1d0d0201d7f8095cf269a71850d9be3fa6b7aa06fb07369c29a32a1f988bf3c7`.
- New final-head hosted checks remain pending; no release evidence is promoted.

## Integration and remaining work

PR162 exact head passed its checks and merged as 84fcc6eac06376cbe52028ef24d0fb67c8236df7.
PR152 and PR153 also merged after green exact-head review. Latest main
278142b349e920e92954f31c9f82ef01a00faa63 had 32 successful, 15 queued and one running
workflow at the bounded observation; branch retirement awaits postmerge checks.
The prior GUI checkpoint aa20106a completed all 40 hosted workflows successfully.
The remaining dependency PRs were automatically rebased by Dependabot and need
new-head checks; checkout PR154 has a retained native_failure child-deadline failure
in run 35094326919, which is not dismissed or bypassed. Issue100 was consolidated
into issue106 without marking unresolved criteria complete. T17, A9 and B6 branches
retain unique commits and remain preserved.

No real Windows installation, rendered recovery action, crash recovery across
all durable boundaries, or release gate is proven by these session fixtures.
Native UI pilot18 remains 0/7 actions and 4/8 captures, Accessibility-untrusted.
All 29 capability states, statements, thresholds, blocking flags and product wording
remain unchanged. Earlier failures and prior evidence remain linked in history.
