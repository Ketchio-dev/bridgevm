# A9 r45 first-logon desktop — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-fe95cb26-ramfb-display-pilot-r45` ran
`fe95cb26b482eb665bb64d19e0a3d2207109d446`, the PR #278 head carrying the 3D-off
ramfb display export that followed [r44](a9-r44-3d-off-display-20260930.md). It
was submitted at 06:56:55 UTC, before that head's hosted checks finished; all
95 of its hosted runs later succeeded, including full project check
`36680741678`. The app was a local exact-source Apple Development build, the
Accessibility preflight and console check passed, and the input manifest
SHA-256 was `907b25b937769aba0b8cf9b32822327ed62fff24b71abb579a85e6a19b78d3f0`.

The job ran on Mac17,9, macOS 27.0, from 06:57:28 to 07:11:51 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`9a0cc327a0590bbd75992bc938f49263c1cfb9996081a5283c17c961f4eaf595`) passed the
strict read check, the lane result SHA-256 was
`500962ad787c2d2d625f5fb9914fc995506f2c843abb1d1ea4b59b5e20549fee`, and worker
cleanup was verified. All 31 half-minute samples found the console unlocked,
and the product app was frontmost in 28 of the 29 taken while the job ran.

Installation, Secure Boot provisioning and first READY each passed once, the
eighth consecutive first READY. For the first time on hardware:
- The runtime logged `ramfb display export: first frame 800x600 published` in
  each of its three processes.
- The display surface presented a frame, and T17 clicked it.
- The click reached the guest as two inserted `POINTERINPUT` requests, one
  event each: the press and the release.
- The keyboard challenge was inserted as 36 events.

The workload still produced no output within 60 seconds. The retained
`display.fb` frame is black, because the packet was collected after the guest
shut down.

## Reading

The retained guest event logs place the input on Windows' first-logon
timeline. The T17 user signed in at 07:10:01. The shell recorded
`WaitForDesktopVisuals`, `SignalDesktopSwitch` and `AfterDesktopSwitch` at
07:11:14.9, then started desktop apps. So Windows switched to the user's
desktop 73 seconds after sign-in. The challenge form wrote its ready marker
when shown, and the click and keys were inserted between 07:10:23.9 and
07:10:25.2, about 50 seconds before that switch.

The leading explanation, grounded in those timestamps but not directly
observed, is that the input went to the first-logon experience rather than to
the form. The form's ready marker proved only that it was shown, not that its
desktop was taking input. r42 and r43 also sent input within about ten seconds
of first READY, so the same cause may apply to them; that is not established.

## Follow-up

The guest form now reports ready only when two things hold: the user's desktop
is the input desktop, and the form is the window under the screen centre,
where T17 clicks. The form stays on top and writes a `clicked` and `typed`
progress line. T17 waits up to 180 seconds for readiness and, if output never
arrives, reports that line after strict parsing. A hosted Windows job
compiles the new user32 bindings and builds the form. None of this has run on
hardware. No journey stage after first READY passed; A9 and A11 remain OPEN
and product state stays ENGINEERING_PREVIEW.
