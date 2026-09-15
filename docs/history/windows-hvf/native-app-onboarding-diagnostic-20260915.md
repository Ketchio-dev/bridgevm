# Native app onboarding and diagnostic preparation — 2026-09-15

Source `1fb301f0c191cc26e2ad6c3c9d0ed0979fef3f5a` includes onboarding `63a1592049be3201350ecba8b431cc7c41e3564c`
and navigation correction `186c41f6f57780a716f477bffd6f3f29236e6b89`.
The first-run screen offers real create/import choices with shared native styling.
Import inputs group VM name, files and saved CPU/memory configuration. Existing
chooser behavior, defaults, media validation, import/recovery algorithms and
runtime ownership remain unchanged. All 172 preceding native test files are
byte-identical. New files are budgeted at their actual sizes; no ceiling rises.

A review found that a published import reached through Overview then Pro-off could
return to welcome after Other Import. The accepted return now selects editable
import inputs. Busy or unpublished transitions refuse before changing state.
The unchanged old transition was run against three new regressions: the affected
test had three assertion failures and the two refusal tests passed. The corrected
run selected 56 tests: **55 passed, one intentionally skipped, zero failures** in
24.83 seconds. The skip is the opt-in native window diagnostic. The app build
passed in 0.56 seconds. A preceding harness compile failure (namespace versus
protocol) is retained separately and is not behavioral failure evidence.

## Native diagnostic boundary

The new d6-app-ui queue tier executes a precompiled, hashed XCTest executable at
its exact sealed commit through the existing GUI physical-Mac worker. It does not
build on the live queue or boot a VM. An owned NSWindow hosts the real ContentView;
empty owned library fixtures disable migration and automatic models. Tripwires
stop before runtime/model/install construction or file jobs. Two overview rows
are synthetic metadata, never activated. The create sheet is canceled; media
selection values must be empty before capture.

Success requires eight owned-view PNGs, seven actual accessibility observations,
zero domain-work tripwires and matching image/report hashes. The runner has a
90-second deadline and bounded cleanup inside the worker's process group. PNG
checking now rejects invalid compressed scanlines, including the independently
retained CRC-correct malformed-image counterexample. Policy checks pass **11/11**;
prior synthetic file-permission fixture failure is retained. Hosted macOS selects
the new onboarding tests and the mandatory opt-in skip; Ubuntu runs queue policies.

At this frozen pre-pilot observation, **no native UI run or screenshot inspection
has occurred**. A default test skip, successful build or policy test proves no
rendering, focus, search interaction or guest behavior. Full project and exact
pushed-SHA checks for this packet are pending. Subsequent pilots must retain failed
attempts and stay app-only diagnostic evidence, with claim eligibility, criterion
pass and capability promotion all false and guest boots zero.

## Previous checkpoint and retained failures

Checkpoint `05ebab4fa5aaa51a9ee1930bde29545d4d69748d` sealed its full project check
in 161.17 seconds. At 2026-09-15 13:21 UTC all 78 hosted workflows succeeded:
[CI 34970421159](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970421159)
and [Security 34970421424](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970421424).
[Run 34970415627](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970415627)
attempt 1 failed at an unchanged Windows fixture's 10-second child wait. Its cause
is undetermined; stdout/stderr were not captured by that timeout path. One unchanged
failed-job retry (attempt 2) passed. Both complete logs remain; the first failure
is not rewritten as success and no deadline/assertion was weakened.

ENGINEERING_PREVIEW and every capability criterion/threshold remain unchanged.
A11 still requires its release regression seal. No installation, Windows runtime,
graphics, performance or release claim follows from this app work.

## Private receipt hashes

These receipts stay outside git; paths below identify retained records, not guest assets.

| Receipt | SHA-256 |
| --- | --- |
| hvf-app-ui-diagnostic-20260915/onboarding-preservation.json | `22f4af9645ded7aab85e604d7407bcc5ef60c034e0379eaf2c865357535a9701` |
| hvf-app-onboarding-20260915/navigation-before.json | `e0859669ca64456e9afd476338f6b3fece52d186f71d20c28ca1e857ec9aad89` |
| hvf-app-onboarding-20260915/navigation-before-02.json | `94da5691872992eb671bcd69005f4ed8978c4980b23faaf6cdec1b005edbab2f` |
| hvf-app-onboarding-20260915/navigation-corrected-final.json | `768fe9801edc28eddac45523a93d857d0d1a08bc8f60260cdda5367d46e4f811` |
| hvf-app-ui-diagnostic-20260915/ui-app-build.json | `ce9bddcc37c5ff9c09f63bb21f94c368ed29cb1e2b99eaa59d3e06af035c2849` |
| hvf-app-ui-diagnostic-20260915/ui-policy-final.json | `b72dacb3e1ca40d239a7cb8a71e096d60be405211376b1282364e8e1d90a1bec` |
| hvf-app-ui-diagnostic-20260915/ui-budgets-final.json | `9a45216ca057426c8c249413ed90001f353ef4b60780f6761d7e3b4a965d1110` |
| hvf-app-design-20260915/hosted-resume-01.json | `abdbb23f3e3a204561c477a1d2687e3f2f79c9c7adf2102bee36606f57f94216` |
| hvf-app-ui-diagnostic-20260915/independent-ui-diagnostic-review.md | `7d3c12ea5b3be248d7a20ce7ceefa0f584dab736ed2974c99b011ab55b60a8cd` |

## Observed native pilots and controller variant — 14:12 UTC

The frozen preparation text above is retained. Checkpoint4aca5518 sealed the
full project check in **163.54 seconds**, log SHA
`093da955ac8fcf67ecfbf8cfca5e5eb23c3fd59b13583cafe3905b617dd58969`.
On Mac17,9 / macOS27.0, app-ui-20260915-01 at4aca5518 loaded the sealed test
bundle but failed the unchanged five-second welcome-control wait. All four
domain-work counters stayed zero; no screenshot existed yet. The next source
002fcbe4 captures before the same wait and retains private structural probes.
Job app-ui-20260915-02 also failed that wait, with four actual welcome PNGs,
zero domain-work counters, zero root AX children and103 native subviews. No
protocol-cast rejection was observed. All four images were visually inspected;
resize/appearance variants show a heading overlay and dark native list text.
These are fixture observations; their product cause is unproven. No actual
create/import/search interaction passed, and no VM was run.

[Hosted CI34978934986](https://github.com/Ketchio-dev/bridgevm/actions/runs/34978934986)
job104414099000 correctly failed freshness at002fcbe4 because new probe source
postdated tested_commit1fb301f0. Complete failed log SHA
`525c21b3d8b5b80e26693940e304510bf9b6232f67b34ff49ae4749cb68aa103`
is retained. No freshness check or criterion was changed or bypassed.

Source `8c149333418eb9c7e35c0e73dc2511b0e4f808f1` includes5ec2778e's extracted search field with native
Command-F focus routing, a32-point clear target and full-name card help.
Filtering and card actions remain unchanged. The controlled test-host variant
retains a public NSHostingController as its window's contentViewController;
the same actual ContentView, dimensions, appearance checks, seven actions,
eight required captures, five-second wait and tripwires remain. Its live
result is pending. The stable selected native run passed55 tests and skipped
the one opt-in window test in24.23seconds, with zero failures and stable source
hashes (log `4bb9633a30071a0080d33968dff8e8e3a262f8cc1d12dcb483043419de48c15f`).
Two earlier compilations overlapped final source cleanup and are retained as
non-frozen receipts, never substituted for this stable run. Current full
project and pushed-SHA hosted checks remain pending. No product state,
criterion threshold, required action or release claim changed.

### Own-process AX and drawing-context observation — 14:20 UTC

Pilot `app-ui-20260915-03` at `d0432679` did not resolve the welcome-control
wait: zero root AX children, 121 native subviews, four captures and zero
domain-work counters. The failed receipt and images are preserved privately
under `pilot-03`. No UI action passed.

Source `d7fd76bef7518138115caa0e84a2843ef9206e07` adds a bounded public Accessibility API reader for this
process PID only, preserving numeric return codes and structural counts
without reading labels/values, changing permissions or issuing UI actions.
Owned bitmap drawing now explicitly uses its effective appearance. These
changes compile with the required opt-in skip in 2.45 seconds, with stable
sources; the live result remains pending. The five-second wait, seven actions
and eight required captures remain unchanged. Current full/hosted checks
remain pending; no criterion or product-state promotion follows.

### Guarded startup observation — 14:31 UTC

Pilot `app-ui-20260915-04` at `928ec177` failed the unchanged welcome-control
wait, retaining four images and zero domain-work counters. Public own-process
AX returned `-25208` (NotImplemented), not `-25211` (APIDisabled); permission
denial is not established. The effective drawing appearance wrapper alone
did not resolve native dark list text. Failed results remain in `pilot-04`.

Source `b2595f26d9292d459b68df319cbc8f313f83ff59` adds a test-only, strongly retained delegate and
public `finishLaunching()` observation. Resolved NSOpen defaults cause refusal;
all document/URL/untitled requests are refused and counted without recording
paths or changing defaults. Public lifecycle states are recorded privately and
the previous delegate is restored. The stable compile with mandatory opt-in
skip passed in 2.55 seconds (log SHA
`c2ffc9ecb1fcaf6b7380cdd47860b8626e423833728a41a774fc4cb5d2fba36f`).
No lifecycle cause or AX correction is claimed before the next sealed pilot.
Seven actions, eight captures, five-second waits and all tripwires are unchanged.
Current full project and pushed-SHA hosted results remain pending.

### Final source and retained native limitations — 14:36 UTC

Source `58d724c3157bcc23eca82918a36fee7290fc7ae9` adds immediate required-name feedback to the
import form using the existing validator's whitespace/newline rule and exact
message. Import stays disabled until a name is present. Bindings, media
selection, worker validation and recovery actions are preserved. The name
component is extracted at 28 lines; the import fields ceiling falls from
46 to 40. No existing structural ceiling rises.

The frozen selected native run passed 51 tests, skipped the opt-in window
diagnostic, and failed none (52 selected; 28.12 seconds including compilation;
log SHA `5182463393c7f239fb0266dc8922ef76143b45e6e31b8eec81d0f4bd9ccbba05`).
The initial budget check saw the deleted bootstrap still in the index and
failed for its absent entry; staging that deletion made the unchanged check
pass. Both receipts remain private.

Pilot `app-ui-20260915-05` at `a9803ab7` refused one document request before
fixture creation. Its public running/finished flags stayed false; will-finish
was received and did-finish was absent. It produced no screenshots or actions.
The request origin is unproven. This failed bootstrap is removed from active
code, preserving its sealed source, binary and complete failed receipt under
`pilot-05`. The preceding gated test entry is restored exactly; the seven
actual actions, eight required captures and five-second waits remain required.

| Pilot | Result | Owned screenshots | Completed UI actions |
| --- | --- | --- | --- |
| app-ui-20260915-01 | Welcome controls unavailable | 0 | 0 |
| app-ui-20260915-02 | Welcome controls unavailable | 4 | 0 |
| app-ui-20260915-03 | Controller hosting did not resolve controls | 4 | 0 |
| app-ui-20260915-04 | Own-process AX returned NotImplemented | 4 | 0 |
| app-ui-20260915-05 | Startup refused a document request | 0 | 0 |

All pilots recorded zero guest boots and zero domain-work tripwires. Images
show actual owned ContentView rendering with synthetic/empty data. Resizing
exposes a top overlay, and dark native sidebar text remains dark in this
host. Their cause and production-app impact are not established. No full
light/dark layout, actual create/import/search dispatch or Command-F success
is claimed. Future UI verification needs a production-style test app lifecycle
and XCUIAutomation or an already-trusted driver; T17's guest-installing runner
is outside this app-only scope. No permissions or private AX flags were changed.

Preceding `928ec177` passed all 80 hosted workflows at 14:33:04 UTC:
[CI 34980910392](https://github.com/Ketchio-dev/bridgevm/actions/runs/34980910392)
and [Security 34980911027](https://github.com/Ketchio-dev/bridgevm/actions/runs/34980911027).
Snapshot SHA `29aed768178cccd4965ff42c829c9cc9c59aef5cc94d318864033ef725db024f`.
Current final full-check and hosted results are pending at this frozen record;
the subsequent exact-tree seal and PR report them separately. No engine
capability, release state, criterion or threshold changes.

### User-directed brand correction — 2026-09-15 14:52 UTC

The user identified the overlapping blue/purple window logo as visually too
close to VMware. Source `da385b25c89447bdad2d185001f43364be02c794` replaces it with a shared
native bridge silhouette: one rising span between two abutments, drawn as a
single solid path. The sidebar and own-HVF emblems use this mark; other engine
emblems use an ordinary single-desktop symbol. Functional Clone/window icons,
VM actions, accessible names and route identity are unchanged. This records
a design response, not a trademark-clearance claim.

`BridgeVMMark.cgPath` is the common geometry for the actual SwiftUI Shape and
the owned vector preview. Pure CoreGraphics rendered the shape at 24, 42 and
64 px on light/dark backgrounds, with no NSApplication or WindowServer. The
preview and SVG are in private `hvf-brand-mark-20260915`, with source/output
hashes in `vector-preview.json`. These are design assets, not app screenshots.
No Dock/Finder icon packaging is claimed; that surface has no current tracked
icon asset or own-HVF icon installation step.

The frozen `BridgeVMControl` build passed in 2.06 seconds, log SHA
`74200c75ad8fd8ebfe4f3fdc9a0ce330e8c53c1faf5d67d66ea5866f25661872`.
New Shape/emblem modules register at 38/26 lines; sidebar chrome and VM card
ceilings fall from 65/106 to 56/86. The unchanged structural check passes.
Current final full-check and pushed-SHA hosted results remain pending in this
frozen record; all previous native UI pilot failures and limitations remain.

The preceding seal `d68bbc60` later failed [macOS CI job104428879101](https://github.com/Ketchio-dev/bridgevm/actions/runs/34983133477/job/104428879101):
`daemon_sends_guest_tools_command_and_tracks_result` observed one pending
command where zero was expected (`part_4_2.rs:193`; 72 passed, one failed).
The actual hosted merge's Rust tree, dependencies and workflows are identical
to preceding green `928ec177`. An unsynchronized response thread and a 25 ms
read window suggest scheduling sensitivity; the failed log does not prove the
cause. Raw/sanitized logs and hashes are preserved with the read-only diagnosis
in `hvf-app-design-20260915/hosted-d68bbc60-failure-readonly-diagnosis.md`.
One unchanged failed-job rerun was requested at 14:51:09 UTC and remains
pending here. No fixture, timeout or criterion was relaxed, and no production
fix is claimed.

The first brand full check failed only the documentation-reference step in
163.21 seconds: an operator HANDOFF line presented a Git tree-object ID where
the prose checker expected a commit. The actual identity remains in the
private seal receipt; HANDOFF now points there instead of presenting a tree
as a commit reference. The checker is unchanged. App build and the other
steps passed; the complete failed log SHA is
`7b03a633d5cede20d447560545f9f3c27077235115ffec844893710c252578f4`.
A complete check on the corrected documentation is required before sealing.
