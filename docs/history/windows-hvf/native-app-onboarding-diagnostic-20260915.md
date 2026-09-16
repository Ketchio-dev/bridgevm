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

### Autonomous app checkpoint — 2026-09-15 16:16 UTC

The user authorized a four-hour native-app improvement block. Cancellation
source `27a0060f` now stays busy in cancelling until its owned validation,
queued pipeline, source-cache verification or child-process wait acknowledges
exit. Acknowledged cancellation is a neutral terminal state; repeated/idle
requests do not dispatch or append misleading cancellation logs. Guards after
awaited boundaries prevent later source/install dispatch, media preparation
and finalization. Existing source-lock and owned cleanup boundaries remain.
A child that has not exited remains cancelling; no timeout/kill policy changed.

The new bounded, no-guest regressions first failed: five tests, 41 assertions,
including actual late-operation counters (red run 7.83 seconds, log SHA
`e9c3d0f21c7b01d0ac7ea009382f4cdcc520318ef44cdd8b62ec0785d3f9ce83`).
The first green candidate's 153-test selection exposed two remaining old
post-cancel state expectations; both still expected preparingSource while the
queue retained work. That failed log is retained, SHA
`f1f0300b6fe99bdd4f4830758206e280255846cfe4a0de9e1d0b3ad5cf3009c5`.
Only those state expectations were corrected; their ownership, job-count,
dismissal/admission refusal and unchanged-file assertions remain. The final
153 selected installation/runtime/readiness contracts passed in 3.46 seconds,
with unchanged sources, log SHA
`ee76a7170451c1db9ba7bd98863936ac1bea53db1dee6ec8bb3d592376127e10`.

Source `400af559` presents all launch blockers, separately all release blockers,
and all product limitations verbatim. Evidence/CTL paths, watchdog, buffered
NVMe and raw command entry move into a default-collapsed advanced diagnostics
component. The original bindings, picker/command functions and accessibility
identifiers remain; the event feed stays visible because it carries failures.
Independent transformation receipts retain exact unchanged action boundaries.

Source `6bc2308b` packages BridgeVM.icns from the exact production BridgeVMMark
CGPath. Pure CoreGraphics/ImageIO produces all ten standard/high-resolution
icon representations; iconutil packages them before existing bundle signing.
[Apple's iconset packaging guidance](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Optimizing/Optimizing.html)
informs this macOS 14+ bundle path. The actual metadata helper passed version,
identifier, icon resolution, ten PNG decode/dimension checks and invalid-input,
existing-file and dangling-link refusal in 0.83 seconds. The 32/512 px generated
assets were inspected. This supersedes the earlier missing-icon packaging
note; actual Dock/Finder appearance remains unverified.

Private records are under `hvf-app-autonomy-20260915`; source identities and
failed/passing logs are retained. No VM, private media or NSApplication launch
occurred in these checks. The prior `9762d9a3` seal completed all 80 hosted
workflows ([CI](https://github.com/Ketchio-dev/bridgevm/actions/runs/34985388961),
[Security](https://github.com/Ketchio-dev/bridgevm/actions/runs/34985388740)).
The unchanged `d68bbc60` failed CI job passed on attempt 2; this does not prove
its earlier scheduling hypothesis or erase the failure. All five native UI
pilots remain incomplete. A normal SwiftUI App/WindowGroup host with its own
public accessibility controls is being developed as a separately sealed
hypothesis; unlike a cross-process driver, it does not require a trust grant.
This checkpoint's full project and hosted results are recorded by the later
exact-tree seal; no product state, criterion or threshold is promoted.

### Normal-app host and packaged resources — 2026-09-15 17:04 UTC

Source `279deaf8` keeps creation inputs in a bounded scrolling viewport while
its title, resource summary, errors and actions remain visible. The library
warning preview now opens the complete existing issue array with selectable
messages and full paths. Source `31985afc` retains the creation sheet during
existing non-cancellable file materialization: inputs and Cancel are disabled,
interactive dismissal is blocked and preparation progress is visible. The
creation/publication/completion functions are unchanged. Actual busy-sheet
Escape and disabled-control behavior remains a native UI observation to make.

Source `774232cf` corrects a confirmed packaging-path mismatch. The generated
SwiftPM accessor searched the app root or a build path, while the packager put
resources in Contents/Resources and deleted its build scratch. The shared
resolver uses the standard packaged location, preserves lazy SwiftPM discovery
only outside an app, and leaves both existing typed missing-resource errors and
trusted-byte validation intact. The actual relocated Foundation-only callers
first reached the forbidden build-fallback tripwire (exit133, 2.309seconds,
log SHA `0ad2c4ae8a2a661cde5acbbbe2997a3306d7fb4a1112a47f9127dd7d77e97497`).
The same six present/missing/root-decoy/corrupt-policy cases then passed in
2.773seconds, log SHA
`5a0a0ac5de02d46381901e5a5ed2987b30a80821b021bf2b99a30b50338b6b48`.
The existing provisioning contract also passed in 15.430seconds. Only owned
copies of the two public resources were used; no app lifecycle or VM executed.

Source `77c9836d781ee42b39a94c9e5301adfd0063bda1` adds a distinct debug-only normal SwiftUI App/WindowGroup
host, using its actual did-finish-launching notification and attached visible
window. Public same-process accessibility must invoke all seven original UI
actions and produce all eight original owned-content captures. A separate
sealed NSWorkspace launcher retains and revalidates the exact app identity;
host completion alone is insufficient without independently observed exit.
The original d6 v1 adapter and observation requirements remain unchanged.

Independent review found that the adapter's six-second termination allowance
could interrupt late ownership plus TERM/exit handling. The worker's existing
five-second group escalation could also move the job folder before external-app
cleanup, invalidating its exact paths. The failed virtual-time regression is
retained, log SHA
`e0546d2fe1b0f87991788c07bd6cf4aec6a19fc8449296d87dc901da38cef903`.
The new tier keeps the launcher in its process group, allows12seconds in the
adapter and15seconds for worker TERM handling in both cancellation and residual
group paths; all original tiers retain their five-second TERM allowance. The
90second execution limit and launcher's five/three/two-second ownership/signal
bounds are unchanged. Unresolved external-app exit fences the worker and retains
the running directory; failed UI with verified app exit remains failed and may
archive. The installed worker must be refreshed while idle to the sealed revision
before the first new-tier pilot.

The final native selection passed215 with two mandatory live-only skips
(217selected, 30.46seconds including build; log SHA
`904bf74373fc6049b33091c1985e4c59dbf4a225a74564ce06791508784eba5e`).
Those skips require WindowServer and an explicitly staged real disk respectively.
Seven isolated host-input contracts passed in7.28seconds. Compile-only release
boundary verification passed in21.23seconds: a positive debug marker, no marker
in the ordinary release binary, and the exact release-flag compiler refusal.
The expanded pure launcher ownership clock contract passed, including owner
arrival at4.9seconds and observed exit at9.9seconds after cancellation. New-tier
contracts passed19, original d6 contracts11 and worker-venue contracts two.
The final staged19-case run passed in2.58seconds with stable index/source,
log SHA `3b7974176232faffc0d992d3f3911e724edb4a4127c4530a75e86763b6773f4b`.

The launcher first failed compilation because its private stored properties
made the synthesized initializer inaccessible; the exact failed log is retained
(SHA `90bfca2f637e8d634e80380f998003e4c5725a9a3186dbdf7f0467a2b7caa93c`).
An explicit initializer repaired it. One wrapper failed before any test ran on
an index-lock collision; another launcher-build invocation was refused for a
missing required option. Corrected invocations passed and neither failure was
rewritten. A staged whitespace check removed one terminal blank line before
the final19-case run. Private records are in `hvf-app-autonomy-20260915` and
`hvf-app-ui-diagnostic-20260915`; source hashes and all failed logs remain.

The preceding `df2e880d` checkpoint sealed a157.88second full-project pass.
Its bounded hosted snapshot observed75of81 workflows successful, six pending
and no failures; this is not an all-green claim. This new checkpoint's required
full project and exact pushed-SHA hosted results follow in its seal records.
No native-host pilot has run yet, the five preceding UI pilots remain failed,
and ENGINEERING_PREVIEW, all criterion states and thresholds are unchanged.

### Full-check integration correction — 2026-09-15 17:18 UTC

The first normal-host full project check failed in131.62seconds solely at the
ordinary XCTest shim compilation, log SHA
`f0233511e42b8a1e522b78e3baa30fad2169834c3ffee8aeafdf8463f1f7c1d6`.
Its textual manifest listed the seven AppUIHostContractTests methods, while
the class was excluded without BRIDGEVM_APP_UI_HOST. Individual Apple XCTest
host success did not override that integration failure.

Source `d1c94b5d970ae383e2fa95a6f2d9da2d88c01d04` moves the unchanged112-line file to the dedicated
BridgeVMAppUIHostTests SwiftPM test target. Its SHA remains
`e060a6cb82100d5230c5c75a53cf9ba1d79f2cbf00a363f6c874eb9556869c11`.
All10 existing target paths/dependencies/types are identical in before/after
package descriptions; only the host target is added. The ordinary shim and
its app configuration are unchanged. A dedicated helper verifies the exact
seven qualified test names before executing Apple XCTest; all seven passed
in19.762seconds including compilation. The separate Swift Testing runner's
zero-test message is not an XCTest result. Workflow and package line ceilings
are preserved by using the helper and an equivalent conventional test path.

Source `63b9e2eb` adds only a synchronous already-working refusal at the first
line of create(), before resetting errors or dispatching a task. Every later
snapshot, mode branch and completion call is unchanged. The existing creation
and policy selection passed14 tests in7.70seconds including build, log SHA
`33787cb9080fff551f583a58be074c0d0ab8471c0c7ce53ab15ef02398f72ca1`.
Rapid native gesture delivery was not reproduced and is not claimed proven.
The earlier compiled host was never queued and is retained as superseded.
The rebuilt final source/manifest identity passed in0.59seconds, log SHA
`31e3c94b4d5252e8d9ebfcfa6c2e221c2d5520566a682123850c4e4f0f2e72d4`.
A complete corrected project check, new release boundary, source seal and hosted
checks are still required. No prior failure or open criterion is erased.

### First normal-App pilot and bounded lifecycle evidence — 2026-09-15 17:42 UTC

The corrected full checkpoint-two project check passed in161.04seconds at
source `d1c94b5d970ae383e2fa95a6f2d9da2d88c01d04`, sealed as
`1541311a8529c450ac47d830e128f9bfe6edfd13`; the stable exact-tree log SHA is
`14c28a3d6b0a6c1b355655e8d1ac60ecfd53722685aa04a4bcb8197e7fcc1a31`.
The initial131.62second full-check failure remains recorded above. The saved
hosted snapshot contains125 distinct executions across45 workflow definitions:
97successful,28pending and zero failures. Forty definitions have two push runs
and one PR run; five have PR runs only. The snapshot does not explain the second
push cohort or prove all-green CI. Pending [CI35000913392](https://github.com/Ketchio-dev/bridgevm/actions/runs/35000913392)
and [Security35000913603](https://github.com/Ketchio-dev/bridgevm/actions/runs/35000913603)
remain unverified in that bounded observation.

The installed worker was updated from97589c0f to1541311a while its clean checkout
and queue were idle under the existing worker lock. The first checkout attempt
refused because that commit was absent from the separate clone; fetching the
exact published revision repaired availability without changing the LaunchAgent.
Normal-App pilot `app-ui-host-20260915-01` ran17:26:22–17:26:37UTC and failed the
unchanged five-second app-launch/window rendezvous. It completed zero of seven
actions and zero of eight captures; all four domain-work tripwires remained zero.
The exact owned app identity and observed exit were verified by the launcher;
fixture and independent worker cleanup also passed. Those cleanup observations
do not make the UI result successful. The failed UI-report SHA is
`5347a6439001fd4c8c6044d133fdb410f80c4bf51cb56e846fdcfccaa0f7276a`.
Original measured running paths remain unchanged in archived receipts.

The generic startup failure did not distinguish the delegate notification,
representable attachment, visibility or content prerequisites. A bounded read
of only that app PID's launch-time framework logs showed window-order activity
but did not establish any missing coordinator flag. No permission denial or
zero-size-view omission is inferred. The previous app already used the same
App.main/delegate lifecycle, so a new custom-main regression is not established.

Source `94ae0a7123c84953456a6764d06b5224e472f22a` adds a permanent private diagnostic-host
lifecycle receipt: seven fixed capped counters, monotonic first-event times and
six current/first-true coordinator flags. It makes at most16 bounded atomic
writes and collects no titles, view content, environment or other applications.
Admission methods were extracted byte-identically except internal access for
the companion file. StateObject factory evaluation remains lazy. Independent
review confirms unchanged callbacks, window attachment and +5second/50ms monitor
bounds; scenario, accessibility and capture files are unchanged. A terminal
coordinator snapshot marks finish begun, while actual cleanup and process exit
remain separate authoritative receipts.

The exact seven host contracts passed under Apple XCTest in19.326seconds, log
SHA `af42a0b5869367d7de1cd3493d6ac76404163d51babc1c2b45c5939f68f6235c`.
An initial boundary-helper invocation refused its missing required arguments
before compilation (0.19seconds, log SHA
`155addde0d45939da48998c9cafcf591d93eb3dc76f11e7ff1fb6f37d20276da`).
The corrected compile-only ordinary/release boundary passed in20.57seconds.
No app launched in these deterministic checks. Private source/binary records
are retained under `hvf-app-autonomy-20260915` and
`hvf-app-ui-diagnostic-20260915/normal-app-host`. Current full-project/hosted
validation and the next sealed pilot follow this source conclusion. All earlier
failures, ENGINEERING_PREVIEW and every criterion/threshold remain unchanged.

### Usability and scene observation checkpoint — 2026-09-15 18:00 UTC

Source18f5ccd4 gives the existing creation resource Pickers actual accessible
names and selected OS/method buttons semantic selection plus checkmarks. Their
options/bindings/actions/identifiers remain unchanged. Source96683e4c moves
installation failure detail below the status row with multiline selectable text
and makes existing log lines selectable. The joint existing creation/install
selection passed82tests in8.57seconds with stable sources, log SHA
`139f71d6ca1740b0107a8e96035f506a00376e833f6ef842a0f6692e71185939`.

Source04ade4c6 returns the existing keyboard submission outcome to the view and
shows an adjacent generic refusal while retaining the draft. Editing or a later
non-refused submission clears the notice. Exactly-empty UI submission is disabled;
whitespace remains valid input. The parent draft binding, existing key actions
and input/send identifiers remain; session/router/protocol behavior is unchanged.
The legacyAttempted path still clears after its unacknowledged write attempt,
including the existing explicit failed-write case. No universal failure-recovery
or guest-delivery claim is made. The existing8draft tests retain all assertions
and now verify all11typed outcomes;21relatedtests passed in7.51seconds, stable
log SHA `a75355cef2f89e5b6919b45e651ecdc058f96cab92d5ee1e3d28a666b99c847d`.

The preceding3a4b7f6a seal passed the full project check in158.87seconds, log SHA
`91a0807505906281999592c9938b02a89e97be86a250e2c31be5070cdcc264e8`.
Normal-App pilot `app-ui-host-20260915-02` at that seal again failed the unchanged
five-second startup bound: App init1, library factory1 and delegate didFinish1;
representable make/update/attachment were all zero. The terminal snapshot came
18.9ms after the five-second deadline. This proves the missing hook observation,
not that no application window existed. All7actions remained false,0captures,
all4domain-work counts zero; fixture/launcher-exit/worker cleanup passed. Its
unaltered lifecycle SHA is
`3ec8bcee643fa1861612f0267d0e3535e2de581027e47f336be6c698620a9db5`.

Source `e8bf70a34c9b5a786137350b13875b2767bd8d8c` adds passive App-body/root-appearance
events, public default-launch Bool/null from the existing delegate notification,
and one terminal snapshot of at most32 own NSApp windows' four boolean flags.
[Apple's launch classification](https://developer.apple.com/documentation/appkit/nsapplication/launchisdefaultuserinfokey)
distinguishes default from other launch reasons; false alone does not identify
a document cause. No new handlers, argv/default mutation, alternate window
selection or0→1frame-size change is made. Original UI factories are extracted
verbatim, ordinary empty background and the diagnostic zero-size hook remain.
Nine fixed capped event counters permit at most18 lifecycle writes, plus one
private window snapshot; no titles/IDs/classes/paths/text or other apps are read.
The same7actions/8captures/5second waits remain required. Seven dedicated host
contracts passed19.316seconds with442Swift files stable; release boundary passed
20.97seconds. Native rendering, VoiceOver and button interaction remain unproven.

The saved3a4b7f6a hosted snapshot has a known failed Coherence inventory check:
[run35003387352 job104497027531](https://github.com/Ketchio-dev/bridgevm/actions/runs/35003387352/job/104497027531)
hit the existing10second native_failure child wait. Earlier PS5.1 and PS7-format
checks passed; later schema/dispatch steps did not run. Full failed log SHA
`aa851b0f5d627043bae355523379a6da9a838f58f23b36f3ee5404b58dd4f0fe`
is retained. Exactly one unchanged job rerun was accepted HTTP201 at17:53:40UTC;
no limit or fixture changed and no result has been observed. A retry cannot
prove the underlying delay cause or erase the failure. Current full/hosted
validation and the next sealed native pilot follow this checkpoint; every
criterion, threshold and product state remains unchanged.


## Native CLI and activation checkpoint (2026-09-15)

Source1aa9352a connects the native vm.json library to `bridgevm app list`,
`inspect` and `readiness`. The ordinary app handles --cli before SwiftUI app
initialization. Native Unicode directory IDs are authoritative; bounded reads
refuse symlinks/special files and never invoke installation recovery. Versioned
JSON reports saved values and unobserved runtime state. Readiness preserves
existing launch checks and separate release blockers. No native start/stop
command is claimed. See [the CLI reference](../../app-cli.md).

The paired Rust CLI uses fixed installed/co-packaged locations, a bounded
regular-file protocol-marker compatibility check, and direct process replacement.
The marker prevents launching an old app that ignores CLI arguments; it is not
a signature check or inode-bound execution guarantee. All149CLI tests passed,
including harmless fixture process forwarding with exact PID/arguments/streams/
exit23, and local/daemon doctor output. Doctor distinguishes store status from
client environment findings; missing QEMU is not an own-HVF readiness verdict.

The previous core descriptor's PROVEN claim was wrong. An actual-registry
regression failed before correction, then all11core tests passed with the exact
ENGINEERING_PREVIEW state/summary. Native parser/reader/readiness18tests passed.
Eight ordinary native CLI processes passed on owned fixture inputs with stable
JSON and unchanged input trees. Three additional paired Rust-to-native calls
confirmed exit0/1/2 forwarding and no library creation. No GUI or VM was requested;
these are query/entrypoint checks, not guest or rendered-UI evidence.

Original legacy-help baseline, native URL-fixture mismatch, CLI canonical-path
fixture mismatch and parallel temporary-name collision failures are retained.
The first complete project check at1aa9352a failed in169.60seconds only on two
new resolver-test cloned-reference lints; all other steps passed. The correction
uses borrowed one-element slices, with no lint suppression or threshold change.
Its later complete rerun is still required.

Normal-App pilot app-ui-host-20260915-03 at09d2846d failed the unchanged five-second
startup/window wait. It observed initialization/factory/body/delegate and default
launch, but no root appearance, attachment or own windows at the terminal sample.
Actions0/7, captures0/8, all four domain-work counts0; authoritative exit/fixture
cleanup verified. No cause or successful UI action is inferred. The next host
records policy before/result/after around the existing activation call. Its
exact7host contracts passed18.79seconds and release boundary passed21.25seconds.
The earlier activation-boundary invocation's mistyped binary path was refused
before compilation; its corrected run passed20.69seconds. No criteria promoted.

A later-turn hosted observation found CI35005133935 and Security35005133799 for
09d2846d successful. The one unchanged retry of35003387352 also succeeded; the
original10second Coherence child timeout remains recorded and its cause is
unproven. Those results do not substitute for this new source's hosted checks.

The next full rerun at3c230091 passed every step except rustfmt in157.16seconds.
The borrowed-slice assertion needed standard formatting. Importing the reused
from_ref helper preserves the same assertion and135-line budget; pinned format
and clippy checks and all7resolver tests then passed. Both failed full logs are
retained. The subsequent complete check is a new observation, not a rewrite of
either failure.


## Owned runtime and installed-name checkpoint (2026-09-15)

The final native CLI full check passed at sourcef522c7a7 in164.60seconds and was
sealed as8575ddf6. Log SHA
`816c3a517e799328005414993ce033bb740a10306d2afbdc0d2eb6283ecadcae`.
Both prior lint/format failures remain failed records. Initial exact8575 hosted
snapshot:85executions/45definitions,62success and23pending; this is not all-green.
The preceding09d2846d later snapshot finished85of85executions successfully,
with120successful checks and1advisory skipped. Those results do not validate a
newer source. The preceding3a unchanged Coherence retry passed while its original
10second timeout and unexplained cause remain recorded.

Source453f218e adds typed runtime launch admission and owned-only stop results,
retains the actual Process until exit is observed, records its actual termination
reason/status, and prevents repeated stop requests extending the180second deadline.
Default GUI attachment remains. Key-delivery failure keeps the process busy until
observed exit. An opaque token identifies only the session's retained child; it
is not authentication, a cross-process media lease or a guest-shutdown receipt.
No new CLI start/stop command is exposed; cross-process preparation races remain.
Independent review passed, followed by122Apple XCTest tests in disjoint89/33
selections. The eight new cases use harmless real children for exit17, SIGTERM,
closed key-pipe failure, duplicate stop and owner-refusal observations. No real
vTPM or guest state was opened. Existing engine/key and native query tests passed.

Sourceb00c0e59 corrects native CLI discovery for the installer's BridgeVM.app name
while preserving BridgeVMControl.app and the canonical co-packaged preference.
The first existing incompatible copy is still refused. All149CLI tests passed
in1.74seconds; pinned clippy passed6.47seconds. No helper override was added.

Normal-App pilot app-ui-host-20260915-04 at8575ddf6 failed the unchanged five-second
startup wait. Policy was regular before/after the existing setter, which returned
false. Initialization/factory/body/delegate and default launch were observed;
root/attachment remained absent and terminal own-window count was0. Actions0of7,
captures0of8; all four domain-work counters0. Exact process identity, observed exit,
fixture and independent worker cleanup passed; no TERM/KILL was required. These
facts do not prove the window-creation cause or validate any rendered layout.

The proposed explicit scene-presentation experiment was invalid: SceneBuilder
rejected the availability if/else in2.62seconds, log SHA
`5c948ed6d59b98e83fd867ce4567831767eea379d81503973ed863f20591d637`.
A separately authorized private two-clause availability typecheck also failed
in2.18seconds, log SHA
`d4da08556d73642785307797d4a94dd5dc290ab77217473e2db33d9659e22634`.
Both failed sources/logs are retained privately. The uncommitted scene helper was
removed and the app source restored exactly; deployment support and all startup
criteria remain unchanged. No new native pilot was submitted for invalid code.
The new full/release checks and exact pushed-SHA hosted observations follow this
checkpoint. ENGINEERING_PREVIEW, all criteria and thresholds remain unchanged.


## Isolated macOS 15 scene experiment (2026-09-15)

A new public-API configuration typechecked with separate compiler targets: the
isolated diagnostic uses macOS15 and the ordinary product retains macOS14. This
is a new experiment after the two rejected availability-branch attempts, whose
failures remain unchanged. The diagnostic helper can now use the public explicit
initial-scene modifier in its compile-time branch. The ordinary branch preserves
the existing titleBar/defaultSize pair, and Package.swift/product metadata stay14.
The diagnostic bundle declares15.0 and its actual linked Mach-O minimum was
verified as15.0. Current physical test host reports macOS27.0; no macOS14 live
behavior is claimed from this experiment.

All7native diagnostic Apple XCTest contracts passed in20.17seconds, log SHA
`c4daca768be2bad789d939a5cbbaad4b5711b22151446b691b78b00383c05485`.
All19adapter contracts passed; the existing fixture body was extracted unchanged
and the bundle minimum check added. A pre-commit whitespace check rejected an
extra blank line in that new fixture module; removing the terminal blank line
did not change its executable body. Required5second startup,7actions,8captures,
ownership/cleanup and zero-domain-work counters remain unchanged.

The preceding ownership sourceb00c0e59 full check passed165.61seconds, log SHA
`bfb0543f97c228dffcaab9f0503e569a0739825cb3774fea9334ad3336a58d9c`,
sealedcb24990a. Its release boundary passed27.88seconds. Initialcb24990a hosted
snapshot:85runs/45definitions,60successful,21queued,4running; no failures observed,
not all-green. The new experiment still requires full/release checks, a sealed
push and one physical-Mac pilot before any window-behavior conclusion.


### Final integration fixture correction

Fullcheck5555ac6b failed in168.33seconds only at doctor integration fixture
creation with AlreadyExists; every other project step passed. Failed log SHA
`f06eb2514c5f2c052153ec0617f6390fca03b8fde53f8527c66f79d5c09e64d7`.
The old timestamp-only allocator failed a controlled equal-timestamp test in
0.57seconds (SHA `4b0a79ee5936b46a6383c1ee53c0a0e4ac3919d5e45ed6b05870a173e92433e6`).
Adding a process-local atomic suffix preserves distinct directories at equal
PID/time. The real-filesystem test also checks no shared file visibility and
that dropping the first fixture leaves the second directory intact.

The related help fixture had the same timestamp-only shape and received the
same repair; no previous help failure is claimed. Both fixtures were extracted
to private test modules to preserve existing structural ceilings. A first help
module compile failed on omitted parent visibility for its path field; only
that visibility was corrected. Log SHA
`418e60e8f89bccf09d5b83165428991d23d43a7fc7c165b1a1686a753f90aa9b`
remains retained. Final150CLI tests passed in0.78seconds, log SHA
`672992bf5c38c288a9b1045245329b29e05ebe2d0e7c819af0da9a79294a4f54`;
pinned format/clippy and independent review passed. No production doctor/help
behavior, process environment, output assertions or timeout changed.

The target15 diagnostic release boundary passed26.82seconds. Direct inspection
of the ordinary release executable confirmed minimum macOS14.0, while the host
minimum remains15.0. Subsequent changes affect only Rust fixture sources and
metadata; the tested Swift inputs are unchanged. A new complete project pass,
seal/push and actual pilot are still required. The four-hour work period has
ended; only this validation and its evidence handoff continue.


## Final pilot outcome and cleanup (2026-09-15)

The corrected full project check passed171.98seconds at75e9b313, sealed/pushed as
ea92823f. Log SHA
`8211a6545b0a623fb64a3fd70e096f071068090f242a66f344c8f1407486a8a2`.
The first exact hosted snapshot had85runs/45definitions:58success,6running and
21queued, with no failed run observed. It was not an all-green result.

Sealed pilot app-ui-host-20260915-05 still failed initial-window readiness under
the same5second/7action/8capture criteria. Its exact diagnostic binary and bundle
minimum were15.0. Initialization/factory/body/delegate counts were1, default
launch wastrue, activation policy wasregular before/after setterfalse, and root/
representable/attachment counts remained0. Terminal own-window count0; actions0of7,
captures0of8; all four domain-work counters0. Exact identity, observed exit and
fixture/launcher/worker cleanup passed, with no TERM/KILL. There are no images to
visually review. Independent analysis SHA
`ac6984db5539964d36ca3d255beb97f7007d11a43972cd8fbc6cb56620c2ab5b`.
This does not establish the missing-window cause; explicit presentation did not
establish any improvement in this run.

The unsuccessful temporary explicit-presentation and target15 settings were
removed. Original host compilation commands and diagnostic bundle minimum14
were restored; actual rebuilt Mach-O minimum14 was inspected. The simple style
and PNG fixture extractions remain so that no existing structural ceiling needs
to rise. Proven native CLI, runtime ownership and equal-timestamp fixture fixes
remain intact. All original failed sources, binaries and receipts are preserved
privately. Restored7host contracts passed18.23seconds and19adapter contracts
passed2.76seconds. No additional unchanged-baseline pilot is submitted. Final
full/release checks and hosted observation follow; UI/VoiceOver/guest/release
capabilities remain unproven, and every criterion/threshold remains unchanged.


## Completion audit and diagnostic packaging parity (2026-09-15)

The final independent audit found that a functioning native UI validation host
remains required. The earlier assertion that only final hosted CI remained was
too narrow and is retracted. All five normal-App pilots remain failed, with zero
of seven actions and zero of eight captures; no rendered UI or VoiceOver pass
is claimed. The previous source was independently reviewed and merged in PR156
only after all85 exact-head hosted workflow runs passed; this did not close the
unfinished UI-validation requirement.

Source `ca1ff9f74dde9889c48d2090dddf522987656b7f` adds only `NSPrincipalClass=NSApplication` to the diagnostic
bundle metadata, matching the ordinary application's declaration. A small module
extraction keeps every existing structural ceiling intact. The existing actual
reconstruction test first failed with `KeyError: NSPrincipalClass` (log SHA
`4a04020698fdc0fcbbf8a76e4bf5464bab0129c8000392c276268212bad4d33a`).
All19 adapter tests then passed in2.72seconds (log SHA
`3f5558f607486d6e305ac7164b008e41553ff27967684e0ae2ff1ad45b3e8ca9`).
All457 Swift/package source hashes still match the sealed diagnostic executable;
its hash and the launcher's hash are unchanged. Every other metadata value,
resource, argument, five-second startup limit, action/capture requirement and
ownership/cleanup guard is unchanged. The missing key is an observed packaging
difference, not an established cause of the window failure.

The first complete project check failed in168.92seconds only on capability
freshness and three operator-document Git-tree identifiers parsed as commits.
Original log SHA
`262e64055fa82661f8145d10eb28affdac3fd16c1b5397e723aee34276d41ece`
is retained. This checkpoint corrects those records; no validator, threshold,
product wording or criterion state changes. A corrected complete project check,
new exact-source hosted evidence and one sealed physical-Mac pilot remain pending.


## Content-construction boundary observation (2026-09-15)

The preceding corrected full check passed166.19seconds and was sealed as45f469cf.
Pilot app-ui-host-20260915-06 confirmed NSPrincipalClass=NSApplication in the
actual reconstructed bundle but still failed initial-window readiness in5.03seconds:
zero owned windows, zero of seven actions, zero of eight captures, and zero domain
work. Exact identity, observed exit, fixture and launcher/worker cleanup passed.
The metadata parity change did not resolve the failure. Its failed receipts remain
retained; the latest hosted snapshot had66success and19pending of85runs, not a pass.

Source 11d446af084fe14aab1153c3d306133f14852432 adds two passive debug-only observations:
root_content_constructed at the existing scene modifier and content_body_evaluated
at ContentView.body. No view wrapper or launch behavior changes. Three exact
extractions retain their bodies while keeping every structural ceiling; the clone
sheet changes only from file-private to internal for its existing cross-file use.
Independent byte comparisons and source review passed. All seven Apple XCTest
host contracts passed20.21seconds, log SHA
`fbe599c9def42ab7f31e9ebedacdb779ee53a1dd5f77d1535f5dc3522b07062f`.
The release boundary passed22.79seconds: positive diagnostic marker, absent ordinary
release marker, and exact forbidden-release compiler refusal. No app was launched.
The rebuilt host and all460 Swift/package inputs are sealed privately.

These counters distinguish construction from body evaluation; neither proves
window admission. The startup limit, seven actions, eight captures, cleanup and
zero-domain-work requirements are unchanged. The new complete project check,
exact-source hosted result and one physical-Mac pilot remain required. Native
rendering/VoiceOver and all existing OPEN criteria remain unproven.


## Minimal-content comparison, never product UI evidence (2026-09-15)

Source b1ca0b5dd3969630f3b1097c5bff5ddaf3431644 introduces a temporary, explicitly compiled launch
control that replaces only the content with minimal Text, preserving WindowGroup,
frame, scene observation, presentation, entry point, library, launcher and deadlines.
It records control_only=true before content construction and intentionally throws
before any product scenario action/capture. This control cannot pass the7/8gate.
The unchanged delegate is extracted to preserve the33line ceiling, now28.

Prior full check passed166.46seconds at11d446af, sealed74807177. Pilot07 failed: both
content-construction and ContentView body counts0 despite App.body/launch completion,
ownedwindows0/actions0/captures0, all4domain-work counts0 and verifiedexit/cleanup.
The exact owned process log reports no persistent state to restore; no root cause
is established. Raw receipts and logs remain private and unchanged.

Normal diagnostic7contracts passed20.13seconds; minimal-control build/exact7contracts/
CONTROL-without-HOST compiler refusal passed26.95seconds, log SHA
`dc3d54ed1bd6ff660e4ba042344f87b2c6ff5f9ed32f2845ab0b9ad1ec7d632b`.
Ordinary release exclusion/exact refusal passed27.34seconds. Source hashes are
unchanged across checks; the control binary/build flags are sealed separately.
No GUI was launched in these checks. Current full/hosted checks and one physical
control observation remain pending. The temporary flag/helper/routing must then
be removed, retaining at most the unchanged delegate extraction for the ratchet.
Actual native UI validation and every existing OPEN criterion remain unfinished.


## Minimal-content result and removal (2026-09-15)

The control-source complete project check passed165.66seconds atb1ca0b5d, sealed
as6abf97e6. Physical pilot app-ui-host-20260915-08 explicitly recorded minimal-text,
control_only=true and product_ui_scenario=false, but failed initial-window startup
before reaching the control's intentional scenario refusal. App/body/delegate were
observed; root construction and owned windows remained0. All7actions stayedfalse,
all8captures absent, all4domain-work counters0. Identity/observedexit/fixture/launcher/
worker cleanup passed. This observation does not support ContentView complexity
alone as a cause; the actual cause remains unproven. This was a control measurement,
not an eighth normal-product UI attempt and never release evidence.

Source 9f9ee4bd998085527e0ef60977a95d05e25cd3df removes the temporary comparison flag, helper and
routing. The product App is byte-identical to74807177 except the unchanged delegate
extraction; scenario content is byte-identical, and the two passive boundary counters
remain. App22lines respects the lowered28line ceiling; no ceiling is raised.
Restored7host contracts passed20.75seconds, log SHA
`50d19aeb95bd5d27998be60675e18ae9ed5ae64b116f5a726314de1f7af929db`.
Release boundary passed22.41seconds. The actual rebuilt host contains the normal
identity marker and no temporary control markers. All461Swift/package inputs are
sealed and unchanged across checks. Failed control code, flags, binary and receipts
remain private and in history; no temporary instrumentation remains in final source.
Current complete project and exact-source hosted checks are required. There is no
reason to repeat the unchanged baseline; actual UI validation remains unfinished.


## Diagnostic request envelope and preparation boundary (2026-09-15)

Source abe1bbcf498f069a2ed7c311bc1b2b8de0c353f8 changes the opt-in diagnostic launcher request
from custom argv to two namespaced environment values. Admission retains the same
canonical private-directory checks and rejects mixed or malformed requests. Before/
after preparation observations read only existing NSApp state and six fixed test/
preview-key presence booleans; no environment values or new application objects are
recorded or created. The product scene, activation, five-second startup, seven actions,
eight captures, domain-work tripwires and ownership/cleanup requirements are unchanged.

The exact seven Apple XCTest host contracts passed20.69seconds, log SHA
`457bafaae1ce605346a6ce74339af455197b00663bc9cb14511face548241f35`.
Compile-only release boundary passed22.60seconds; all four checked diagnostic keys/
markers are present in the host and absent from ordinary release. Rebuilt launcher
and pure ownership contracts passed5.82seconds. Inputs remained unchanged, and the
host/launcher pair is sealed privately. These checks launched no app or guest.

The preceding restored full check passed165.80seconds, sealed as a23d0f92. The saved
PR157 snapshot records84of85workflow runs successful; final CI35026414196 remains
pending in that snapshot. Coherence run35026411053 succeeded on unchanged attempt2;
the original failure remains retained and its cause is unproven. Seven normal-App
pilots and the separate minimal-content control remain failed. Current full-project,
exact-source hosted verification and one sealed actual-product UI pilot are pending.
This request-envelope comparison is a hypothesis, not an established startup repair.
All criterion states, thresholds and product wording remain unchanged.


## Actual window admission and presentation observation (2026-09-15)

Packet35's complete project check passed169.87seconds, sealed240db0be; the22:10UTC
snapshot recorded all40push workflows successful. Normal-App pilot
app-ui-host-20260915-09 reached one visible product window, root/body/attachment/
appearance callbacks and scenario start, then failed the first light/default
compound presentation guard at about483ms. Actions remained0of7 and captures0of8;
all domain-work counters were zero and identity, observed exit and all cleanup
passed. Preparation recorded NSApp absent before and after, environment transport
with zero custom arguments and all six fixed test/preview-key presence flags false.
This measured startup improvement is not a full UI pass or proof of an argv cause.

Source f5a46ef7ee5037bdc7118bd6fcfeecf98acafb7f extracts the existing presentation method and
records bounded geometry, appearance and content identity only when its unchanged
compound guard fails. The sizes,200ms sleep, appearance assignment, layout pass,
guard operands/tolerances and original thrown error are preserved, including when
the private receipt cannot be written. Coordinate spaces are named separately;
no presentation repair or cause is inferred before actual values are measured.

All seven Apple XCTest host contracts passed19.94seconds, log SHA
`3428d82495a65da5a2f709d31801d04a3bede4740640e2e0dd317b82187484da`.
Compile-only release boundary passed22.24seconds. All466Swift/package inputs
remained unchanged; the Packet35 launcher is unchanged. Independent review passed.
PR157 completed85of85successful workflows and merged as36ff324d with the a23d0f92
tree unchanged. New main CI35029792177 was pending in its22:13UTC initial snapshot.
The preceding timeout and successful unchanged retry remain recorded; cause is
unproven. Current full-project and exact-source hosted checks plus one sealed
pilot10 remain pending. All29criteria, thresholds and product wording are unchanged.


## Diagnostic application appearance source (2026-09-15)

The preceding complete project check passed164.17seconds, sealed654a8050. Pilot
app-ui-host-20260915-10 admitted the actual product window and scenario, then failed
first light/default presentation. Its private record shows content identitytrue and
exact1320x860 bounds, with explicit window/content appearancesnil and effective/
best-matchDarkAqua for requested light. Only the appearance predicate is false at
record time. Guard and observation are not atomic; the underlying mechanism remains
unproven. Actions0of7/captures0of8, zero domain work and verified identity/exit/cleanup
remain preserved with the failed source, binary and receipts.

Source b00d38c8d30e5c3ffbd82549b0edfedeac22385b requires an existing NSApp and nonnil named
appearance, then assigns that appearance to the owned diagnostic app and window.
The process-local override remains through subsequent captures; ordinary/release
behavior and persistent settings are unchanged. The extracted failure observer adds
only bounded app appearance fields. Requested sizes,200ms wait, layout, all original
guard predicates/tolerances and mismatch error remain intact, as do the7/8requirements.

All seven Apple XCTest host contracts passed20.03seconds, log SHA
`e282fc968e49d16c3aed430a4531e61a6a3f66530b40ee682ef41ec6a9b6e7b4`.
Compile-only release boundary passed22.04seconds with all467Swift/package inputs
stable. Independent inverse-diff/API review passed. These deterministic results do
not establish the candidate's effect. Current full-project and exact-source hosted
checks plus sealed pilot11 remain pending. Merged main36ff324d CI35029792177 remains
pending in the last recorded snapshot; no new poll was taken for this checkpoint.
All29criterion states, thresholds and product wording are preserved.


## Minimum-size constraints observation (2026-09-15)

Packet37's full project check passed163.25seconds, sealedadd08e00; all40push workflows
passed in the saved22:37UTC snapshot. Pilot app-ui-host-20260915-11 produced one
validated light/default PNG, visually reviewed by the owning agent, then failed
minimum presentation: requested1100x720, original/current bounds1100x772 and
layoutRect1100x720, with identity and requested Aqua appearance matching at record
time. The underlying geometry mechanism remains unknown. Actions0of7/captures1of8,
zero domain work and verified identity/exit/cleanup remain preserved. One reviewed
image does not establish complete UI operation or minimum/dark presentation.

Source dc8694ca63133ff2b1b1212ed8a3d386fd322568 adds failure-only size-limit, fitting/intrinsic,
instance-conversion and original-content layout-constraint observations. Each axis
records at most16numeric constraints; no hierarchy, descriptions or text is captured.
New nonfinite numeric observations encode asnull while finite intrinsic sentinel-1
is preserved. The extracted rectangle helper is unchanged. Setter, sizes,200ms wait,
guard/tolerance, app/window appearance, product scene and7/8requirements are unchanged.

All seven Apple XCTest host contracts passed19.84seconds, log SHA
`a3b62b3420196a9e8085ebcb8100e5dcb262ef63aa11b3f0799bbb3716104115`.
Compile-only release boundary passed22.09seconds. Both receipts contain468stable
Swift/package inputs; independent diff/API review passed. This observation code has
no measured geometry result yet and is not a presentation repair.

Merged main36ff324d records44of45successful workflows, including CI35029792177.
Coherence35029792045 failed its unchanged10000ms native_failure deadline; exactly
one unchanged retry requested22:46:24UTC is pending. Original failure logs remain
retained and cause is unproven. Current full-project, exact-source hosted checks and
sealed pilot12 are pending. All29criteria, thresholds and product wording remain.


## Independent presentation observations with retained failure (2026-09-15)

Packet38's full project check passed164.42seconds, sealed106addcf. The22:59UTC
snapshot records its40of40push workflows and mergedmain36ff324d45of45workflows
successful. Coherence35029792045 passed its single unchanged retry attempt2;
the original10000ms timeout remains preserved and cause is unknown.

Pilot app-ui-host-20260915-12 remained FAIL: actions0of7/captures1of8, zero domain
work and verified sealed identity, exit and cleanup. Its published window/content
minimum height772 exceeds requested720; recorded frame/content conversions retain
dimensions. This establishes an incompatible requested minimum, not the layer that
sets it. Neither product layout nor the diagnostic minimum criterion is changed.

Source 3ddf1191cf1a172f2dd81142307e1670209b9256 observes the same four variants once in their
original order. Only the dedicated presentation mismatch can continue, after task/
marker cancellation and exact visible owned-content readmission. Failure is latched
and explicitly saved before continuation, failed captures are skipped, and an atomic
private matrix preserves at most four rows. Matrix/report persistence, capture,
ownership and other errors abort. Existing first-failure sidecar behavior remains
best-effort and no-overwrite. The fresh light/default guard and entire seven-action
suffix are byte-identical; success still requires all7actions/8captures without failure.

Exact seven Apple XCTest host contracts passed21.28seconds, log SHA
`95520ca4abd9b14817f02e4b985a52689998bd329a604edd26763f87ecba907d`.
Release boundary passed23.73seconds; both receipts contain474stableSwift/package
inputs. Independent control-flow/inverse-diff review passed. Pure callback contracts
prove neither rendered UI nor a repaired minimum. Current full-project, exact-source
hosted checks and sealed pilot13 remain pending. All29criteria, product wording and
thresholds are unchanged; earlier failures remain recorded.


## Sidebar contrast candidate and owned accessibility observation (2026-09-15)

Packet39's full project check passed192.45seconds, sealed77da5b86; all40push workflows
passed. Physical pilot app-ui-host-20260915-13 at that seal remained FAIL, with
0of7actions/2of8captures. Both default presentations produced verified images; both
minimum variants recorded height772 for requested720. The actual dark image shows
near-black native-sidebar labels with poor contrast, followed later by a welcome-
controls AX timeout. Neither geometry nor AX cause is proved. Zero domain work and
verified identity, observed exit and cleanup remain recorded with all prior failures.

Source 8cd6365a5faa8d5596d866cd22bed0585a0fa344 extracts the existing sidebar List and applies semantic
Color.secondary to its two same-copy section headers and Color.primary to the HVF
label. Selection, tags, menus, order and actions are preserved. Actual visual repair,
selected-row contrast and high-contrast behavior remain unproved before new captures.

Only after the existing welcome timeout, a diagnostic observer checks cancellation
and exact visible content ownership, then records bounded full-protocol and separately
labelled public-selector trees. Each tree is limited to256nodes, depth32 and128children
per node; only fixed allowed IDs/roles and bounded type/structure fields are retained.
No labels, values or alternate action route are introduced. Recording failure preserves
the original timeout. The5second/50ms wait, action walker, minimum dimensions,7actions/
8captures and report/completion policy are unchanged. Framework getter duration remains
subject to the unchanged outer launcher bound; the receipt is non-atomic.

The first compile failed after7.10seconds on an incorrect public-protocol type name;
its log SHA `dafdb36bd2a1b2880de9237e894ba5060fcc75e2220477358c7f4daee38e49c4`
is retained. Only two type references were corrected before all seven Apple XCTest
host contracts passed20.66seconds, log SHA
`4628df69881c0fbec60a2646d286cea0bdf5274fca85ef16a14e7b9911cbaaf5`.
Release exclusion/refusal passed28.59seconds. Both receipts contain480stableSwift/
package inputs; independent inverse/privacy/bounds review passed. These deterministic
results prove no rendered correction or AX cause. Current full-project, exact-source
hosted checks and sealed pilot14 are pending. All29criterion states, product wording
and thresholds remain unchanged; the minimum and complete UI criteria remain open.


## Owned descendants and supplemental window observation (2026-09-16 UTC)

Packet40's full project check passed in 187.65 seconds, sealed as 7a5d6431. Physical
pilot app-ui-host-20260915-14 remained FAIL, with 0 of 7 actions and 2 of 8 captures.
Both default PNGs are byte-identical to pilot13: the explicit semantic styles did
not repair the observed dark-sidebar contrast. Both minimum variants remain 772
points high for the required 720. The strict and public-selector timeout trees each
contain one full-protocol AXGroup root with empty children, no rejection or
truncation, and no allowed IDs. This does not establish why the root exposes no
controls or whether accessibility exists elsewhere. Identity, exit and cleanup
passed with zero domain work; all failures remain recorded.

The private timing labels were corrected: actual scenario start was 0.7768 seconds
after host begin, within the 5-second startup deadline. The 7.0606-second duration
from host begin to termination includes scenario work; it is not startup duration
or evidence of a startup overrun. This correction does not change the failed
minimum-size or welcome-controls result.

Source 8621d37a288741776360693829de845f6512abfb removes the three ineffective foreground styles
and restores the original List source while retaining its extraction. After the
existing root AX timeout record, diagnostic-only observations traverse owned
subviews, bounded to 256 nodes, depth 32 and 128 children per node. One shared budget
limits AX metadata to 128 entries. Ordinary and navigation counts remain separate;
no discovered entry becomes an action route. Immediately after the existing
dark/default bitmap, the same bounded view projection records appearance only,
without AX queries or extra layout/display calls.

One supplemental window image follows through ScreenCaptureKit currentProcess,
restricted to macOS 14.4 or later in the diagnostic build. Exact unique window ID
and nonnull own PID, finite pixel bounds, explicit child-window/shadow/cursor/audio
exclusions, ownership and cancellation checks around awaits, and exclusive output
in a private 0700 directory are required. No global fetch, permission prompt, picker
or fallback is added. This image stays outside the eight required captures and
cannot supply missing actions or minimum-size images. API activity may affect
framework state and timing; the records are non-atomic and the existing owned
launcher bound remains authoritative.

All seven Apple XCTest host contracts passed in 25.06 seconds, with log SHA
`0522927c99b981e260b2d93c8049abbe2bf6f4e24ffac18a2e72bad7846b83c1`.
Release exclusion/refusal passed in 23.50 seconds. Both receipts contain 491 stable
Swift/package inputs; independent review of all 17 source/test files passed.
Original 5-second waits at 50 ms intervals, 200 ms presentation timing, dimensions
and tolerance, seven actions, eight captures and completion policy are unchanged.
These checks prove no rendered repair, AX cause or supplemental capture success.
Current full-project and exact-source hosted checks, plus sealed pilot15, are
pending. All 29 criterion states, product wording and thresholds are preserved.


## Navigation-observation trap and bounded recovery (2026-09-16 UTC)

Packet41's full project check passed in 178.03 seconds, sealed as e7a3d57c.
Physical pilot app-ui-host-20260915-15 remained FAIL, with 0 of 7 actions and 2 of
8 required captures. All four matrix rows persisted; both minimum variants remained
772 points high for the required 720. The two default PNGs and supplemental owned-
window PNG passed integrity checks. Final completion and own-window records were
missing, however, and the empty fixture directory remained. The original failed
checks are retained: verified launcher exit and process cleanup do not establish
host completion or fixture cleanup. All four domain-work counters stayed zero.

The retained crash binds PID 32450 and binary UUID
`a23cdf37-4169-31d1-bceb-d64429d76955` to the exact sealed host. It records a
main-thread SIGTRAP through Swift array type checking and Collection.prefix,
AppUIHostOwnedAXEntries.project at line 12, and the navigation-order projection in
AppUIHostOwnedViewFields at line 23. The concrete offending element and assertion
reason are unknown. Crash log SHA
`7832c89846e1188b75de63c30496de819c142dd96168b400e8d0059d532d503a`
and all partial artifacts remain preserved; the complete-fixture verifier still
rejects this run.

The owning reviewer compared the original dark bitmap with the supplemental
WindowServer image. The composed window shows a light HVF label and blue icon,
while cacheDisplay shows that row near-black. This sequential, non-atomic comparison
does not support another product foreground workaround or establish all contrast
correct. Both default PNGs and the supplement are 2640 by 1720 pixels for default
content of 1320 by 860 points, agreeing with the observed scale of 2. The previous
run used scale 1; its change remains unexplained. The first light bitmap preceded
the supplemental API phase. The lifecycle record is explicitly nonterminal:
0.2607 seconds measures startup, not the mislabeled fallback terminal duration.
The original analysis and a separate timing correction are both retained.

Source c298474318512cd166826453756141d85f889a7e removes only the navigation-order
getter and projection from the diagnostic timeout observer. Private owned-view
schema 2 explicitly marks navigation unobserved with a fixed reason; no empty array
or alternate selector pretends to observe it. Ordinary bounded metadata, ownership,
privacy, cancellation, all waits, dimensions and tolerance, seven actions, eight
captures and completion policy are unchanged. This removes the identified trap
route without claiming a framework cause, complete AX repair or actual UI recovery.

All seven Apple XCTest host contracts passed in 21.89 seconds, with log SHA
`010abb72178b990e19a7f66515cb9eb30bb9f55a06f2562e18018fd291af9b71`.
Release exclusion/refusal passed in 23.48 seconds. Both receipts contain 491 stable
Swift/package inputs; independent two-file inverse review passed. Current full-
project and exact-source hosted checks, plus sealed pilot16, remain pending.
All 29 criterion states, thresholds and product wording are unchanged. Earlier
failed experiments and incomplete completion evidence remain preserved.
