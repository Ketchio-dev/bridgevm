# A9 and B6 continuation: bounded repairs, not release promotion

The operator resumed development for forty minutes at 11:46:58 UTC on
2026-09-10. This record separates deterministic repairs from live criteria.
A9, A11 and B6 remain OPEN; product state remains ENGINEERING_PREVIEW.

## A9 chooser recovery

The previous pilot `t17-f35bac2d-local-pilot-r6` stopped before AXRaise because
an advisory activation attempt returned false. The chooser now attempts
activation but does not treat that Boolean as a necessary precondition for
AXRaise and PID-targeted keyboard delivery. Panel discovery, exact path
readback, enabled Open, panel dismissal and final selected-path equality
remain required. Ten T17FileChooser tests passed locally; these do not prove
the packaged UI path. A fresh ad-hoc app was packaged and its exact helper
was re-registered in Accessibility. No release signing claim follows.

## Native Windows tip query

Hosted run 34473588674 retained the actual provider tree: the visible owned
`Got it` control had name `Got it`, IsOffscreen=false and ControlType.Pane.
The original query correctly refused a non-Button. Explicit fixture role and
system rendering did not change that observation (34473714000); that failed
attempt was removed, not relabelled as a pass.

Explicit client-provider registration from PowerShell raised a
NullReferenceException (34473884595). Registration through a non-inlined
concrete C# method returned a real Button and authenticated physical point
(34474006030). Its fixture still failed despite correct present output;
retaining the child process handle before waiting fixed exit-status handling.
The final code checkpoint is `28a57da5de62ac68240c0938e537d366208afd5f`.
Hosted run 34474091572 passed all four caption and five tip native contracts,
plus the host parser tests. Tip cases cover an enabled unique owned button,
disabled button, wrong name, duplicate buttons and a hidden root.

Provider registration uses the documented
[ClientSettings API](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.clientsettings.registerclientsideproviderassembly?view=netframework-4.8.1).
The upstream [ProxyManager implementation](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/UIAutomation/UIAutomationClient/MS/Internal/Automation/ProxyManager.cs)
inspects reflected caller types while loading default proxies. The dynamic
PowerShell stack is the supported working explanation for the initialization
failure, not a claim that the installed .NET Framework binary was symbol-traced.
No Button/owner/visibility/bounds checks were relaxed. A not-found response
still does not prove the absence of a visually rendered guest popup. No live
guest popup result is established by these hosted synthetic windows.

## Checkpoint and retained failures

The installed worker is now documented by its actual script SHA-256 rather
than a Git blob identifier that the document checker interpreted as a commit.
The first resumed local full check failed its cancellation receipt SHA check
because HEAD changed during that check. A second check with stable HEAD
passed all other steps but correctly rejected the stale capability registry.
The registry review pointer is updated separately, without closing A11; a
fresh complete local check and exact hosted checks remain required.

The separate `codex/b6-renderer-trace` diagnostic branch passed seven new
trace-policy contracts, fifteen existing cell contracts and a full local
implementation-worktree check. Hosted trace run 34473807985 passed. Its first
CI 34473855644 rejected stale registry freshness; this is not green release
evidence. The submitted job `d3-b6-ebb94c8c-renderer-debug-r3` seals its source,
probe, renderer and fixed logging policy. It must find actual TGSI and GLSL
records before reporting a valid trace, and all promotion flags stay false.

## Subsequent continuation

Classification repair `cf465e5d` passed the full local project check, hosted CI
34533860375 and Security 34533860483. This closes the document-registration
omission, not A11's final release regression criterion.

A9 diagnostic branch `7cc7148d` passed full local checks and exact hosted CI
34475122806 / Security 34475124672. Pilots `t17-7cc7148d-local-pilot-r7` and
`t17-7cc7148d-local-pilot-r8-observed` both failed waiting for the Go To location
field; both lane results reported cleanup_verified=true. During r8, read-only
AX observations and a screenshot showed the ordinary Open panel, not Go To.
No observer click or key was added to that pilot. A separate isolated manual
interaction could show GoToWindow / PathTextField and return the exact ISO
selection AXValue, but this does not establish helper event delivery.

The next diagnostic records bounded activation, frontmost PID, AX status,
window role/identifier and focused-element metadata before/after PID-targeted
keys and at timeout. It never records titles, input values or path strings;
unknown role/identifier values are classified as other. Four focused context
and privacy tests plus ten existing chooser tests passed. Transport and
selection acceptance conditions are unchanged. A new live result is required.

The renderer prefix repair independently reanalyzed the retained 10,162,048-byte
capture log with its original SHA-256 unchanged: TGSI273 / GLSL273. This is
explicitly offline reanalysis. The original diagnostic-failed receipt was not
rewritten, and no glyph/performance criterion or product state was promoted.

## Twelve-hour continuation: checked AX conversion and valid renderer trace

Main f1d4a887 passed full local checks, CI 34536775123 and Security 34536775119
after replacing the new forced AX cast with the existing CF-type-checked
conversion pattern. The earlier bbfe7ecc forced-cast failure remains a failure.
The rebuilt helper and its manifest authenticate locally; its Accessibility
grant is not established because the computer-use native pipe failed to start.
No new A9 pilot has been run and A9 remains OPEN.

Physical job d3-b6-eedd819d-renderer-debug-r4 finished at 22:05:54 UTC.
Its receipt is valid, outcome observed, failure_code none, run_count 3 against
required_run_count 27, with pass/criterion_pass/claim_eligible/promotion false.
Receipt SHA-256: 345e16b1d7a8c2d4d3895d702345479d5bb4f7d863a27ca4d4d3bdd4e65c5f.
Capture trace SHA-256: 661c0081337152d5834c28f55ece9e310f072852c41884164fb1fcef7861f5f8.
The capture has 273 TGSI and 273 GLSL headers in 10,015,877 raw bytes. This
proves capture/parser compatibility, not glyph correctness or performance.
Code 22868620 integrates the already-tested diagnostic without changing the
renderer binary or any criterion. Its 25 focused contracts and complete local
project check passed; exact integration-head hosted checks are still required.

Visual inspection of the packaged Notepad body shows coloured/incomplete
strokes both in this diagnostic and in the non-instrumented 568e1544 renderer's
retained d2-b6-a399e341-1600x900-100-r1 capture. This is not an accepted glyph
mask and is not a causal identification. The title, tab and menu are readable
in these particular frames; that does not discharge the fixed matrix.

A narrower arithmetic hypothesis comes from the retained r4 shader: TGSI
`UADD TEMP[1].x, -TEMP[1].xxxx, IMM[1].wwww` uses an integer constant 8, but its
GLSL applies `floatBitsToUint(vec4(-temp1).xxxx)` before unsigned addition.
Offline IEEE-754 evaluation for x=0..7 yields 2147483656..2147483663 rather
than 8..1. This is a static expression mismatch, not a GPU or live guest proof.
Mesa's TGSI source-modifier documentation specifies type-dependent integer
negation: https://docs.mesa3d.org/gallium/tgsi.html#source-modifiers .
A minimal real-translator reproduction and an isolated candidate are the next
steps; no blending rewrite or product-renderer cutover is justified yet.

## PPM evidence parsing and candidate discovery failure

A one-second bounded baseline reproduction found that the mask builder loops
on a truncated PPM header. With a CRLF header and raster bytes
`0,64,128,192,255,17`, it returned `10,0,64,128,192,255`. Code 1f8e1d20 shares
one exact parser with the verifier and uses the digest from that same byte read.
Eight boundary/hash-consistency contracts, existing glyph/scene self-tests and
the full local project check passed. No reference is thereby reviewed or
certified, and the existing mask/sample/frame-time criteria are unchanged.

The private integer-negation candidate passed six real-translator output
contracts; its baseline failed the four integer cases and passed both float/
untyped controls. The existing six sampler inference/precedence cases passed
on the candidate. These remain deterministic translator checks, not GPU proof.

Job d3-b6-791c68e3-integer-negate-r1 failed host preflight because the manifest
renderer path differed textually from the probe's load command. The bytes were
not a live guest result. A new, separately sealed manifest used the exact load
path for r2 without changing the library or the check. R2 passed scale setup,
then failed capture: managed UI Automation FindAll returned E_UNEXPECTED
(0x8000FFFF) during packaged-Notepad tip discovery. Only the classic first
capture exists. The candidate's packaged-body effect is therefore unmeasured;
no glyph fix is claimed. A same-head original-renderer control is queued/run
separately to distinguish the shared discovery failure from renderer changes.
