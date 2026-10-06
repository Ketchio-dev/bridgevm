# Strict timer receipt fixtures — 2026-10-05

This tests-only successor preserves the strict T1 receipt validator and repairs
fixtures that still expected historical unbound timer results to be accepted.
No timer algorithm, threshold, guest contract or capability state changes.

## Retained failed integration

Source f59bd55ffd8f3ac7ec690cb07f33cd2a117a65f0 failed its local full check:
43 PASS, 1 FAIL in 721.771 seconds. The failed product/A19 stage expected a bare
T1 receipt to pass the CLI. Raw log SHA-256:
`91e48fe709975da1654ce1c519043d85cf1ebf79a9f52eb7db72addc1fc2d388`.
Required hosted full run37272230040, job111641482091, also failed that fixture:
42 PASS, 1 FAIL, conditional cross-compile SKIP. Raw log SHA-256:
`53dd392eccd265b3219c8b437d6abf2e77e4f1840eb799d1c4c9b5f698720e66`.

Exact push T20 run37272226487 and PR T20 run37272231429 reproduced this failure.
PR security/quality run37272231111 also failed because its redaction fixture
put an A3 receipt under a T1 job identity. The new strict reader correctly
rejected it. These failures remain failed; F59 is not admitted for live work.

Manual CI37272232936 reported workflow success with13 successful jobs and one
failed nonblocking advisory job: GitHub DNS checkout failed before compilation.
PR314 had147 successful,3 failed and1 skipped check at the retained checkpoint.
D11 push37272226650 and PR37272231581 checks passed separately, including53
Python methods and7 mocked cases under each hosted Windows PowerShell shell.
These narrower successes do not override the failed full check.

The root execution interruption also left the local300/600-second inspections
unperformed. No retrospective timely checkpoint or continuous work is claimed.

## Fixture correction and coverage

A3 redaction now uses its own matching queue/ledger identity. Copying that valid
payload under T1 still fails with empty output. Original T1 submission, claim
and cancellation checks remain. Generic compatibility uses a non-T1 tier over
running/done, full/short commits and absent/empty/matching optional ledgers.
Its ledger-mismatch case first proves the same fixture works before changing
only the ledger commit. Bare T1 results remain rejected in the same12 cases.

Complete T1 producer/publication evidence also passes the actual CLI in both
queue states, each without and with a matching ledger. Changing only published
configuration then fails with empty output. This closes a review gap: the
earlier positive test called the Python reader directly and did not prove CLI
routing. The original direct-reader and publication checks remain intact.

Author9e71502d integrates as7f6cfceb. Focused checks pass103 policy assertions,
8 snapshot CLI methods and15 T1 methods, plus budgets and whitespace checks.
Independent source review and root41-pin verification cover seven test/support
and budget files. Production readers and TimerFixture remain exact F59 bytes.
Extracted helpers reduce existing ceilings; none is raised.

## Remaining verification

This successor still requires its own complete local and exact-SHA hosted
checks. No physical timer run, Windows installation, performance improvement,
audio-quality result, release evidence or criterion promotion is claimed.
