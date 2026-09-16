# Native diagnostic ownership — 2026-09-16

Classification: historical deterministic evidence. Current product state belongs
to the [registry](../../../capabilities/windows-hvf.json). A11 remains OPEN and
the product remains ENGINEERING_PREVIEW. This record follows the
[native owned-stop checkpoint](native-owned-stop-deterministic-20260916.md).

## Integration and source

PR160 was merged after all 85 exact-head workflows succeeded at
`1eb6a81e8281dbb0256c683eaea579f590e07f7c`. Its main merge is
`4b80760887eac7166b37a0573bee50a79c7d083d`; the last bounded inspection saw
34 successful and 16 queued/running post-merge workflows, including maintenance jobs.
That main was integrated without changing the native-stop source tree.

PR159's main merge passed all 50 recorded post-merge workflows. Its obsolete
remote and local branch were deleted after checking ancestry and worktree use.
The original dirty development folder, unique diagnostic branches and sealed
source worktrees were preserved. No operator file was staged.

Chooser source commit: `ff10caeeea1cf907362e1675c30fabbb800fdc06`.
Owned-output source and final frozen project receipt are identified in the private seal.
Final source identity and input hashes bind the focused and full-check receipts.

## Failures retained

The previous native-stop push at `a88136da32a38f969e0bece47eba19c45c09f0c0`
had 39 successful push workflows and one failure. This is not a complete
pull-request or release validation result. B6 native-tip run 35058765563 observed
child exit zero after about 1.03 seconds, then refused the still-shared stdout
file. Its summary nevertheless said owned cleanup was confirmed.

Earlier B6 runs 35054007363 and 35054007189 at the cancellation checkpoint also
failed output capture. The latter first passed all 73 headless checks under
Windows PowerShell 5.1, then failed an original UI case. Those are separate results;
a successful headless phase does not rewrite the failed workflow.

The evidence proves that child exit was insufficient for output ownership.
It does not establish why another Windows handle remained open. In particular,
the current PowerShell Unix redirection callback is not evidence of the Windows
5.1 mechanism. The fix owns the pipes and writers explicitly instead of assuming
an opaque redirect has finished when the child exits.

Resident inventory run 35054007215 failed its existing ten-second child deadline.
The same test/module/workflow bytes later passed run 35058765714. The specific
historical timeout cause remains unproven. Source review independently found
wait-before-drain, unbounded reads and kill/dispose/delete without observed exit.
Those defects are corrected without raising the deadline or claiming that the
historical failure was caused by pipe saturation.

The original chooser diagnostic test expected a wrong-role element to return nil.
Running that unchanged test produced one failure among eight cases. The existing
selector intentionally throws a blocker; the correction expects that exact
blocker and matchError, preserving the selection and refusal rules.

The first resident regression run failed because its synthetic disposal callback
lost the enclosing factory variable. The fixture now captures that callback
explicitly; this failed test is retained and was not a product or guest result.

## Changed contracts

Owned query capture retains the exact process, both pipe readers and bounded
file writers. Query completion needs child exit, both EOFs, and flushed/closed
writers. Each stream stores at most 65,537 bytes, retaining one overflow sentinel;
acceptance remains limited to 65,536 bytes. Full observed byte counts are separate.
Raw bytes and existing strict UTF-8/UTF-16 BOM decoding are preserved.

B6 keeps its twenty-second query deadline. The inventory wrapper requests its
existing ten seconds through the shared adapter. Cleanup has a separate single
five-second bound, shared by child and stream observation. A timeout cannot become
success because cleanup finishes later. Unconfirmed owners and raw evidence stay
retained; only bounded confirmed snapshots may enter failure artifacts.

A process-lifetime registry reserves active/unresolved runs before side effects.
Admission stops at sixteen retained runs without evicting any owner. Confirmed
cleanup releases a reservation; scope exit, capture errors and disposal failure
cannot discard unresolved handles. Constructor rollback preserves both errors.
Independent review found and closed the earlier local-scope retention gap.

The inventory harness keeps its nonzero-exit, empty-stdout and nonempty-stderr
acceptance conditions and original three provider-error cases. It reuses the
owned process/output implementation rather than maintaining another pipe loop.

Chooser diagnostics record fixed labels and counts from the existing production
predicate lookup. Missing, not-yet-attempted, wrong-role and interrupted lookups
remain distinguishable. Callback counts, roots, roles and original errors are
preserved, and combined redacted context remains bounded to 900 characters.
This context supplements timeouts; immediate exceptions keep their existing detail.
No Accessibility query, actual chooser interaction or VM was launched for this work.

## Validation and limits

| Check | Observation | Scope |
| --- | --- | --- |
| Original chooser predicate tests | FAIL, one of eight tests | Original failed expectation retained |
| Corrected chooser families | PASS, 32 XCTest plus 19 Swift Testing cases | 45 source/test inputs unchanged; synthetic graph only |
| Frozen full project R1 | FAIL, 212.345 seconds | Only operator HANDOFF tree reference misread as a commit; all code checks passed |
| Final owned-output R4 | PASS, 147 checks in each of File and dot-source modes | All seven inputs stable; PowerShell 7.6.6 only |
| Resident owned-child R4 | PASS, 48 lifecycle assertions plus three original cases | All seven loaded inputs stable; original ten-second bound |
| Frozen full project R2 | PASS, 210.505 seconds | All 3,130 tracked inputs unchanged, including operator HANDOFF; HEAD and index unchanged |

Private evidence is under the development checkpoint directory. Prior failed
logs and snapshots remain intact; new results do not replace them.

| Record | SHA256 |
| --- | --- |
| Hosted B6 synthetic failure archive | `0779a714e8a663e942ec1da21d64659523c0ad4a41d7316915767851997e53c2` |
| T17 original-tests-r1.log | `baa612a724d75177dd3a697a0a26238407532f0fe592aef9237ad1b1b4d52398` |
| T17 corrected-tests-r2.log | `1a843fe0f039166526bf8292545475962e8600d92c9a485f745c64bafdb56ac1` |
| Frozen project-r1.log | `fb74b9775f85feef2706c5f68702389f80e6d0ae4477c3402f5934e7799c65e1` |
| B6 file-r4.log | `2173366821f3dbb3f24e0a2b2cb6a5be95ba6b75698df35d8aeefa117832af3b` |
| B6 dot-source-r4.log | `9f632a143d451b4dead4febe891a746a91f76901b05178bc8c133c5b1b23acff` |
| Resident focused-r4.log | `95598bb9b0674cf6505ca2a1579eb158f00b36a7f62ab80ca1656652fe23633e` |
| Frozen project-r2.log | `5ea4eca4583935fbcb8dd0df73eb9887f1972a1e7ec1610d0698952b96c2f4b5` |

The hosted inspection interval was 05:17:41–05:20:41 UTC. Pending run IDs and exact
snapshots are retained for the next turn. No later hosted status/log query was made.
Corrected Windows PowerShell 5.1/UIA checks, final post-push CI and live native
app/guest stop remain pending. Native UI pilot18 still lacks trusted Accessibility.
CLI start, guest application flush, supervisor crash recovery and release sample
counts remain unproven. All 29 criterion policies and product wording are unchanged.
