# T17 diagnostic timing and failed required checks — 2026-10-03

Source `159eac2a2dd296e12dbb2d14c4a0e7b2aaf17fc7` failed its required
local and hosted checks. The earlier [failed Windows pilot](t17-runtime-chooser-pilot-failure-20261003.md)
and [shared-budget source conclusion](t17-shared-chooser-budget-and-return-timing-20261003.md)
remain unchanged. This checkpoint adds no live result or criterion pass.

## Actual local failure

The complete project check ran from 14:20:40 to 14:33:40 UTC, exited one after
780.407 seconds, and recorded 44 outer headers: 43 PASS and one shim-suite FAIL.
All 4,053 tracked source fingerprints matched before and after the run.
Finite progress/load checkpoints were recorded; the complete raw log is retained.

The ProductE2E shim target recorded 304 passed, three failed and zero skipped.
The failed methods are the existing diagnostic-context, empty-context and
bounded-context checks: two exact-text expectations and the old total-length expectation.
The actual message preserves the original stage and context, followed by the
new complete timing envelope; the bounded-context fixture records 1,493 instead
of 972 characters, including a 521-character timing footer.
The original driver context still clips at 900 characters.

## Exact-source hosted failures

Standard run 37129293315 ended with eleven required SUCCESS jobs, two required
FAIL jobs and an advisory SKIP. Both macOS app jobs, 111221007011 and
111221007013, record the same ProductE2E 304/3/0 result. The authenticated PR
merge tree matches the failed source tree.

Hosted full run 37129404957, job 111221334370, failed after confirming the exact source marker, at the shim suite.
It executed 43 outer headers: 42 PASS and one shim-suite FAIL, with the same
three diagnostic failures. Linux cross-compilation was explicitly skipped for
an absent target; three conditional native shim cases also remained skipped.
Those unexecuted cases are not counted as passing headers or guest evidence.

The five Portable/Security/T20 companion runs passed their required jobs.
They do not override the failed required app/full checks. The optional CGL
probe in standard job 111221007004 records error 10002/exit 101; the required job succeeded
through continue-on-error. The original failures and raw receipts are preserved.

## Test-only compatibility repair

Reviewed source `6cab5646896fcf94a1ece5e771884173c2fe80ff` retains all four
original diagnostic test methods and their owned no-IPC timeout/driver fixture.
It checks the original message and context exactly, including the unchanged
900-character context clip and its 899/900/901 boundaries. It separately checks
the complete trailing timing envelope, its 900-character limit, last-completed
stage, failed-stage timing and absence of the private fixture path.
No-advance sentinels still reject typing or confirming without a ready field.

The unchanged original four-test baseline actually records one PASS and three
FAIL. The repaired focus records 60 PASS with zero failures/skips; the entire
ProductE2E shim target records 307 PASS with zero failures/skips. Selections
are not summed. Actual env.sh sourcing, commands, toolchain and unchanged
inventories are retained. These are native Swift repository-shim results.
Production source and crate subtree objects are byte-unchanged by this repair.
Existing test ceiling lowers from 56 to 38; new support files register at actual
31 and 26 lines. Every legacy two-column budget row keeps its format.

Full local and exact-source hosted checks for the successor remain required.
No 159e app was built/signed, worker switched or pilot submitted. No new
Windows run proves the chooser cause or repair; A9, A11 and A19 remain OPEN.

- Failed local raw SHA: `4793e6712bcbf28ffbf5fc54b53148525cced4252cff9062778ed5d141e6fb6b`.
- Failed hosted-full raw SHA: `7426033dfda4b64d357e962bf5058b2788658492a5d9acb9f00aa11eb1696c7e`.
- Test-only source handoff SHA: `0d3c9485cf672a2a2564a621c35839144341f66df7850d5e24e811b2fad840da`.
