# Post-merge integration, round 7: a timing-sensitive display test fails the 1db70ab8 hosted full (2026-10-24)

Draft PR #428 (`agent/followups-r2-20261023`), continuing
[round 6](followups-r2-integration-r6-20261024.md). Evidence ranks follow
AGENTS.md: *live single run*, *automated*, *static reasoning*. Nothing here
promotes a criterion; A5 stays PROVEN, A9 and A11 stay OPEN.

## 1. Seal `1db70ab8` checks (automated)

- Local full: PASS, all 45 stages, 03:01:54–03:19:46Z, clean tree (log
  SHA-256 `6dac47fc…`); shim suites 430 / 1469 (3 skipped) / 65 / 454;
  only the seven expected A19 negative-test FAIL lines.
- Hosted full: run 40056629747 (job 120760927593) **FAIL**, 44 stages run,
  one failed: "tests (probe example)", 726 passed and 1 failed (log
  SHA-256 `26b94633…`). The other 102 workflow runs for `1db70ab8`,
  including CI's own probe example tests, succeeded.
- The failing test, `the_deadline_exports_only_while_no_request_comes`
  (added with the pulled display export), reported "42 ticks for 40
  requests" against its bound of 41. The exporter's stop line in that test
  shows `seconds=2.4` for a loop of 40 requests 10 ms apart, which takes
  about 0.45 s on an idle host (*host log observation*). `1db70ab8` adds
  only records to `43007ca0`, whose hosted full passed this test.

## 2. Cause and fix (`e27a490f`, automated)

- The exporter re-arms its 100 ms deadline at each request, so it exports
  on the deadline whenever a whole period passes with no request. The test
  assumed its 10 ms sleeps never stretch to a period; a loaded host can
  stretch them.
- Reproduction: under background QoS (`taskpolicy -b`), the test at
  `1db70ab8` failed 10 of 10 runs (103–115 ticks); idle, it passed 10 of
  10.
- The test now counts, for each window from just before one request's
  write to just after the next (or the count), the whole periods in which
  nobody asked, and allows one deadline export for each. With no such
  window the bound is still 41. It passed 10 of 10 idle and 10 of 10 under
  background QoS; the probe example suite passes 727.
- It still catches the defect it guards: with a request leaving the grid
  deadline in place it failed 5 of 5 (45 ticks, 0 periods unasked); with
  each request advancing the grid it failed 5 of 5 at "the deadline
  stopped". The other three request tests passed 5 of 5 idle and under
  background QoS without change.
- The test now uses the exporter's own `uptime_ns`; its only unsafe site
  is gone (unsafe ceiling 1 → 0, LOC unchanged at 198).
- Product code is unchanged: the source differs from `43007ca0`, where
  T17 r95 passed, only in this test and in records.

## Not verified

- That the hosted extra tick came from a stretched window: that run kept
  no per-request times (*static reasoning* plus the 2.4 s stop line). A
  torn read of the request word would also cost one tick; the fixed test
  does not allow for that, so a recurrence with 0 periods unasked would
  point there.
- Every item listed under "Not verified" in round 6 stands.
