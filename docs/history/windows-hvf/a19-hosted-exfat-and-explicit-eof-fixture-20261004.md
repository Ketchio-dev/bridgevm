# A19 hosted exFAT and explicit EOF fixture checkpoint — 2026-10-04

This records deterministic results for two different source commits.
A19 and A11 remain OPEN; product state remains ENGINEERING_PREVIEW.
No live criterion or release head is designated.

## Preserved d88 results

Source `d88ce4244c341814cac7ca7cc4252e3c43b839e4`, tree
`3ba2351e4e2b28497bcfc1dfdc533ef88f00b969`, belongs to
[draft PR 306](https://github.com/Ketchio-dev/bridgevm/pull/306).
Its mandatory local project check returned 1 after 825.787730 seconds:
44 outer stages, 43 PASS and one FAIL, with 4161 source records unchanged.
The failed workspace invocation ran 29 hvf-runner tests: 28 PASS, one FAIL.
`oversized_truncated_and_late_frames_fail_without_allocation_growth` reached
`owned_protocol_transport_tests.rs:19`: `assertion failed: rejected`.
The original failure is retained; its historical cause remains unproven.
Local raw SHA-256:
`0825ee56bb8a6f22823e10adef7d43717f9260ad4d01c58afbf7ba0929a813a1`.

Exact-source hosted closure independently recorded 105 successful runs,
146 required successful jobs and one source-expected advisory skip.
All 147 actual check IDs matched the source-derived job IDs; 30 required
core raw logs were retained and reviewed. The other 116 required jobs
were checked through terminal metadata, without claiming complete raw audits.
[Manual full run 37185314098](https://github.com/Ketchio-dev/bridgevm/actions/runs/37185314098)
[job 111385899821](https://github.com/Ketchio-dev/bridgevm/actions/runs/37185314098/job/111385899821)
had 43 applicable outer PASS stages and terminal `project check: PASS`.
The source declares 44 stages; Linux cross-compilation explicitly skipped
because that target was not installed. All 111 unittest summaries had
unique OK terminals. Nine nested negative-fixture FAIL labels remain recorded.
Three native opt-in skips and the optional CGL advisory failure remain limits.
Hosted success does not override the mandatory local failure.
Full hosted raw SHA-256:
`496c422833c255ab761b244cb10070d4ade1223b0e015090a149d249febb9376`.

## Genuine hosted exFAT fixture

[Filesystem job 111385900061](https://github.com/Ketchio-dev/bridgevm/actions/runs/37185314098/job/111385900061)
ran eight no-device selftests with OK, then one owned synthetic filesystem
fixture: 24 commands, four cases, 42.192582 seconds, success and verified cleanup.
Actual volume metadata identified writable exFAT; the owned detach returned zero.
Only metadata and command logs were uploaded: 71 files, 72,100 uncompressed bytes.
Image, synthetic pair and compiled helper bytes were excluded.

An 8191-byte quota refused the 8192-byte pair without staging or destination
change. Empty and absent destinations published at quota 8192; both product
verification calls returned zero. Complete nonempty replacement returned 1
with `Operation not supported (os error 45)`, preserved the exact old destination,
and retained complete new staging. The errno does not identify a failing syscall.
These are synthetic filesystem contracts, not a ten-lane Windows lifecycle,
interruption, power-loss, security, guest or A19 criterion pass.
ExFAT raw SHA-256:
`0d1c39535f475cabaf35322eef93c85465d9c962bc9f8d54059b30661c762ffd`.
Uploaded archive SHA-256:
`f326809f370367a6808339fe200ed395dc30eae022217fcc76dcb6eb7d3b521a`.

## Explicit EOF fixture successor

A deliberate owned UnixStream witness kept a cloned write handle while dropping
the original. The truncated vector remained Pending for all four permitted reads;
the negative focused invocation returned 101. Explicit write-half shutdown on
the same input rejected on the third read; the positive invocation returned zero.
This isolates a synthetic EOF premise, not a reproduction or cause of d88's failure.

Source `1b19fec859ab0ddedab247471106940c160d046e`, tree
`f345c24ab8c9d49f8b1f6fe2bd134a239bddd28d`, seals the three-file fixture repair.
The test retains an owned cloned writer and explicitly shuts down its write half.
The existing late-frame assertion block moves byte-for-byte into a test helper.
Production algorithms, vectors, four-read limit and bounds stay unchanged.
All 29 focused runner tests passed; formatting, budgets and diff checks returned zero.
The transport ceiling decreases from 29 to 21; the new helper is registered at
its actual 13 lines, with no existing ceiling raised. Temporary instrumentation
was removed. This local source commit is not yet pushed at this checkpoint.

Final source-plus-history/budget metadata sealing, mandatory full-project checks
and exact pushed-SHA hosted verification remain PENDING. Their outcomes are not
borrowed from d88. All 29 criterion states, statements, release flags, known defects,
thresholds and user-facing capability wording remain unchanged.
