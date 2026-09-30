# A9 r48 Start menu not in the foreground — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-a7219612-flyout-dismiss-pilot-r48` ran
`a7219612863920b7943cc6e69f2cf9391040c5fe`, the PR #278 head carrying the
first-logon flyout dismissal that followed
[r47](a9-r47-start-menu-20260930.md). Its hosted Windows challenge contract
passed before submission. The app was a local exact-source Apple Development
build, the Accessibility preflight and console check passed, and the input
manifest SHA-256 was
`90cd400a9875e73537dcfd3933da29624dee62c3ba93b917e74c64dcd20fe158`.

The job ran on Mac17,9, macOS 27.0, from 08:23:47 to 08:39:28 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`de90255284fa47d43f3b26a3acff22725a402116e21ad0696ac0a7160e3e78c0`) passed the
strict read check, the lane result SHA-256 was
`c283f6df6545e8ea7f23b1a52224ff9945583af9dcaaf802ded339c084d57ac8`, and worker
cleanup was verified. All 33 half-minute samples found the console unlocked,
and the product app was frontmost in all 31 taken while the job ran.

Installation, Secure Boot provisioning and first READY each passed once, the
eleventh consecutive first READY. The ready marker came 46 milliseconds after
the form's previous progress line, at the desktop switch. The display click
was inserted as two `POINTERINPUT` events and the challenge as 36 events. The
form reported `clicked=0 typed=0 session=2 integrity=high foreground=other
cursor=400x300 dismissed=0`, and the post-READY host diagnostic stop completed.

## Reading

The frozen `display.fb` frame and the final ramfb frame (SHA-256
`e764026e5c920aec66ae9645e7c2fa98429b040d23d6014ab473470f455ed178`) again show
the Windows 11 Start menu open over the challenge form, with an empty search
box. The form's dismissal never ran: it looks for the Start menu, search or
shell process in the foreground, and none was there.

The empty search box points the same way. Had the Start menu held keyboard
focus, the 36 typed characters would be in it. So at first sign-in Windows
shows the Start menu over other windows without making it the foreground
window, and a check of the foreground window cannot see it. The Start menu
may also have opened just after the ready check; r48 does not separate the
two.

## Follow-up

T17 now clicks the challenge form beside the Start menu, as a user would to
close it, and then clicks its centre; each click needs its own inserted
pointer receipts. The click spots are placed inside the letterboxed guest
image rather than the whole display surface. The guest-side dismissal stays
in place but is not relied on. None of this has run on hardware; A9 and A11
remain OPEN and product state stays ENGINEERING_PREVIEW.
