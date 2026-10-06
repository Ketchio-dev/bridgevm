# A19 stop-point fixture transition and preserved failures — 2026-10-03

**Evidence rank: automated tests and static review.** Failed sealed checkpoints
remain failed; the new repair's complete local and exact hosted checks are pending.
This record adds no live result, criterion pass or capability promotion.

## Failed sealed checkpoints

Original source: `dbedf4dfbff761a2b4874b9ab4c4416f663d30f3`.
Its mandatory LOCAL executed 44 ordered outer steps: 43 PASS and one rustfmt FAIL.
Raw SHA-256: `86e950f0e04de358c4ae7e482cd03179f0a8d3f92ecead8bba40b90281969d38`.
Its [manual hosted full](https://github.com/Ketchio-dev/bridgevm/actions/runs/37116837149)
(job 111185180992) executed 43 outer steps: 41 PASS and two FAIL, Windows
product/A19 contracts and rustfmt. The absent Linux cross-target was skipped;
three explicit native opt-in/debug tests also remained unexecuted.
Raw SHA-256: `a1b99a6132144f63f4f7ea94fe0c35bdd227f37a7d29b72891f944dec7cc82e3`.
The [standard workflow](https://github.com/Ketchio-dev/bridgevm/actions/runs/37116790121)
ended CANCELLED: ten required SUCCESS, one Rustfmt FAIL and two app CANCELLED,
plus one advisory SKIP. Failed job 111185048371 raw SHA-256:
`e9e5ef5dcee069ffe75b26e63a56035848fad324af2da9ae2f7f7dffc4e0fc1e`.

Formatter-only successor: `c14a3ee4cbbb051a32942f944fc290ae56661e46`.
Its mandatory LOCAL passed all 44 ordered outer steps, with the same three
explicit unexecuted native/debug/live tests. Raw SHA-256:
`891b0ee78f1e5bbe9d67440270c5e302c3f07d4c6e9e755fdd13aa5afcca9db0`.
Actual required [T20 push](https://github.com/Ketchio-dev/bridgevm/actions/runs/37117879552)
(job 111188094608) FAILED: the eight-test interruption-case suite reported one
create-before-manifest negative-subcase error. Raw SHA-256:
`f8ffcb9293eaed0575e895bf7c2f168409aa7a0501e2a2c8c2235f69fb36338f`.
The LOCAL success does not override that required hosted failure.

## Controlled fixture diagnosis and bounded repair

Integrated source: `0469fe3bf7065e95be383902e6af01f942cb1c90`.
The controlled baseline reproduced exact `RuntimeError`:
`staged read or old selection changed at stop`. The negative fixture assumed
only timeout, although the production observer also refuses a state transition
between its precheck and file-descriptor observation. Actual hosted timing was
not observed; the controlled reproduction does not establish the actual hosted ordering.
The negative helper could temporarily qualify before selection/manifest changes
during descriptor observation. Controls now change selection before stage creation
and write the invalid manifest first, before staged-read preconditions can hold.
Three real-lsof transitions require exact refusal, child reaping and no affirmative receipts.
Production observer logic, stop points and deadlines remain unchanged.
Focused local checks passed 17 suites / 114 tests; independent replay passed
11 tests. The affected local subset also passed 11 tests; overlapping replay
counts are not added to the 114-test aggregate. Combined log SHA-256:
`2cfc446ad5fbf25ab999c5814e8d960e0cfe6dddf35938e7fda727dba6a9b12f`.
It is assembled from three untruncated merged tool-output chunks, rather than
a directly redirected raw stream. Independent replay raw SHA-256:
`a4c645e4fb3679fa47d3c82a0ce5303375322b69eed34c35519b57b1686b1c30`.
Controlled baseline raw/state SHA-256, respectively:
`a4df5b8020da7e5d867ae0f11d3fd75e0a55ad839b23d70fcd3a0523cf06fe0e`,
`ee4163ce59c36e82cbced60c969cf44bfcabd9cfbaca41c4be27234bb0e12b86`.
Formal source handoff SHA-256:
`20a9749735028b1ff62e14b5867fc09b325c6e102904b5523ada3eadff050242`.
These focused checks do not replace complete exact-source local/hosted checks.
They preceded integration on matching source bytes; no complete compiled-input
inventory, exact successor full result or new hosted success is claimed.

## Other failed evidence and unchanged thresholds

The c14 optional CGL [job 111188156274](https://github.com/Ketchio-dev/bridgevm/actions/runs/37117882150/job/111188156274)
reported four pixel-format error 10002 lines and exit 101. Raw SHA-256:
`62eb781c9c4461c6a963d28ab392d72189d636bde225ca9d62ee25bf91b66964`.
Its advisory job status SUCCESS from continue-on-error is neither a graphics
PASS nor a SKIP; this repair does not override or diagnose the actual failure.
The genuine owned exFAT attempt built its helper and created its disposable
image, then attach failed with permission denial.
Zero snapshot publisher cases ran; mounted filesystem type, publication and
parent sync were not tested. Permission-denial cause remains unknown.
Frozen handoff/independent audit SHA-256, respectively:
`fad1c92b7084a6d310797a3bb1014f80102a115a140a97f45e2492d90d321845`,
`4fc77cf9754427a8feaf7beea7091bdfe6852a384ecf892efd025a66319a87eb`.
A19 still requires all ten lifecycle lanes passing 10/10 at the release head;
quota, pair authentication and cleanup requirements are unchanged.
No Windows run, exFAT publication, power-loss proof or release acceptance is claimed.
