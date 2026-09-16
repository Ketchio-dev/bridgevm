# Native diagnostic callback scope — 2026-09-16

Classification: historical deterministic evidence. Current product state remains
in the [registry](../../../capabilities/windows-hvf.json). This follows the
[owned output record](native-diagnostic-ownership-20260916.md). A11 stays OPEN;
ENGINEERING_PREVIEW and all release requirements are unchanged.

## Hosted failure and integration state

PR161 head `87b747aac6926f09467b45da76dddb2740f92af6` had 63 successful,
six failed and sixteen queued/running workflows at the last bounded inspection.
The six failures are push and pull-request executions of three Windows suites:

| Suite | Push run | Pull-request run |
| --- | --- | --- |
| B6 native tip | 35062197814 | 35062235376 |
| B6 caption focus | 35062197890 | 35062235372 |
| Resident inventory wire | 35062197994 | 35062235183 |

All six failed before their query deadline. The first actual harmless child
could not call Get-B6TipProcessObservation; cleanup could not call
Wait-B6TipProcessCompletion. Exit and output drain stayed unconfirmed.
The earlier PID17 capture/deadline messages were intentional fake cases, not the
actual failing child. Original B6 UI cases were skipped after the headless failure.
No bounded failure artifact was uploaded; the failed logs remain retained.

The adapter used GetNewClosure callbacks containing unqualified helper names.
Those callbacks did not retain the complete helper dependency graph. A nested
script-scope local reproduction also loses those names after its creator returns.
The previous portable top-level passes did not exercise that scope boundary.
This does not explain the older opaque-writer sharing failure or inventory timeout;
those separate historical causes remain unproven.

PR160 main merge `4b80760887eac7166b37a0573bee50a79c7d083d` now passed all
50 recorded post-merge workflows. Its local and remote runtime-status branch
were deleted only after confirming the exact merged head, zero unique commits,
and no worktree use. PR161 remains unmerged while its new-head checks are pending.
Original dirty worktrees, historical branches and sealed evidence are preserved.

## Correction and evidence boundary

The adapter retains the actual receive, observe and wait callable implementations
before returning. Every callback edge uses those retained values, including
Observe, WaitExit, Dispose, observation-to-receive and wait-to-observation.
Caller helper replacement or creator scope exit cannot retarget those calls.
No helper is exported globally and no fallback performs ambient name lookup.

Process identity, both retained streams, strict capture, EOF/flush/close proof,
query/cleanup deadlines, byte limits and the sixteen-run retention cap stay intact.
The resident harness still uses ten seconds and its original three error cases.
The B6 query still uses twenty seconds and the original five UI cases.
No guest, Accessibility interaction or actual native Windows UI ran locally.

## Validation

| Check | Result | Scope |
| --- | --- | --- |
| Scope regression before correction | FAIL, 4.932 seconds | Helper shadowing reproduces lost scope; failure and confirmed cleanup retained |
| Corrected B6 File and dot-source modes | PASS, 155 checks each | All eight inputs stable; portable PowerShell 7.6.6 only |
| Resident focused entry | PASS, 48 lifecycle assertions plus three original cases | All seven inputs stable; unchanged predicates and deadlines |
| Independent final review | No actionable blocker found | Retained full call graph, real scope regression and exact twelve source hashes |
| Frozen full project check | PASS, 211.526 seconds | All 3,132 tracked inputs, HEAD and index unchanged; operator references included |

| Private retained record | SHA256 |
| --- | --- |
| Scope regression red log | `7ebea28a74a8372a420bdcba2b2d2d531ef975dea50de543ec62a4f7ce3d2b7e` |
| Corrected B6 File log | `9879877753204dea1b551089ed9ca8da408101abb42b5ec4aa686392da057b76` |
| Corrected B6 dot-source log | `5d4d28c474cb0ef6881aab8ab458154dd4ef24124fe0862492c8927214e34942` |
| Resident focused log | `f53f8a0a39dbde1611ecc6649e241ac5d1dba218850d8633202a809746fddc8c` |
| Frozen full project log | `2c7ac987bd98cbca76ddeae5d727f4331681bf18a43ce123c373d073dfc8d5f5` |

The private checkpoint holds exact source, log and input hashes. Failures remain
in the record and do not become passes when a later correction succeeds.
Hosted inspection opened at 06:08:25 UTC and closed within its three-minute bound;
remaining job IDs are saved for the next goal turn. Corrected Windows PowerShell
5.1 and native UIA results still require exact-head hosted validation.
CLI start, guest application flush, live app/guest stop and supervisor crash
recovery remain unimplemented or unproven. No criterion is promoted here.
