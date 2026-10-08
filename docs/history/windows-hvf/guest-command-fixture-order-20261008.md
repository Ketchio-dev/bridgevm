# Order real guest-command fixture writes before observation — 2026-10-08

Evidence rank: synthetic deterministic tests and static review, not guest/live.
Hosted manual CI37833134359, job113503317657 (macOS26), actually failed the
daemon command/result test at pending count1 versus expected0:81PASS/1FAIL.
A server-thread250ms delay before the mount reply reproduces the same assertion
locally. This proves a scheduling assumption, not the hosted delay's exact cause.
The first local filter selected0tests; corrected fully qualified invocation
ran one and failed. Both logs and the original hosted failure are retained.

A reconcile drains currently available messages with its existing25ms idle
read timeout; asynchronous command dispatch does not promise an already-read
reply. The old test required that without first establishing the server write.

The test-only peer now follows real protocol order on one thread: bind listener,
production connect with no hello, nonblocking accept, actual authenticated hello
write, production reconcile, API command dispatch, actual peer read/assertion,
actual result write, then one reconcile and the original strict metadata checks.
Accepted peer is explicitly blocking with finite read/write bounds. No session
is injected and no protocol handler is called directly. A second scenario
withholds each result through an idle reconcile: pending stays1, no share is
published and prior result stays unchanged, then actual write permits pending0.

A direct child blocks on fixture-owned stdin rather than shell/expiry timing.
Its state guard exists before spawn, success asserts production cleanup, and
unwinding closes the peer and kills/waits the owned child before root deletion.
A real panic-path test confirms process absence and owned-root cleanup. No
server thread, detached join, retry-until-pass or production deadline change.

Root focused2PASS; removing the production drain loop makes both result tests
fail pending1!=0. Mutation removed, production source byte-identical. Restored
whole daemon84/0 and all-target Clippy pass. Extraction/budget ratchets preserve
all original request, result, capability, payload and approved-share assertions.
Exact sealed local full and hosted checks remain required; original failure is
not erased by other passing checks. No live/Windows or criterion promotion.
