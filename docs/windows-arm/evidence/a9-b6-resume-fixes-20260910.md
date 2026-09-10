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
