# A9 r46 desktop-ready input — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-b8f1dd2b-desktop-ready-pilot-r46` ran
`b8f1dd2bfe5257f2b755f9d18d73df1deb11fd52`, the PR #278 head carrying the
desktop-gated input challenge that followed
[r45](a9-r45-first-logon-desktop-20260930.md). Its hosted Windows challenge
contract passed before submission, and all 95 of its hosted runs later
succeeded, including full project check `36684043822`. The app was a local
exact-source Apple Development build, the Accessibility preflight and console
check passed, and the input manifest SHA-256 was
`0bccb5ad84a92d15d3e4a83a4501f1d70389c2e5f01dab1fc0f5fd57fc4e9749`.

The job ran on Mac17,9, macOS 27.0, from 07:33:06 to 07:46:33 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`f9b2262a9cd87672afde13e4d9ec1322eef22638cee5f54fa11362f149d45fb7`) passed the
strict read check, the lane result SHA-256 was
`32536020fc534593958e7c0f7b166b9f6a10eb34387ece098b0eb0ebe4176438`, and worker
cleanup was verified. All 30 half-minute samples found the console unlocked,
and the product app was frontmost in 26 of the 27 taken while the job ran.

Installation, Secure Boot provisioning and first READY each passed once, the
ninth consecutive first READY. The form wrote its progress file when shown
and its ready marker about 49 seconds later. The ready marker coincided with
the shell's `AfterDesktopSwitch` at 07:44:46.5, so the new gate held input
until Windows showed the user's desktop and the form was the window under the
screen centre. After that, the display click was inserted as two
`POINTERINPUT` events and the challenge as 36 events. The lane failed with
`guest workload did not produce t17-keyboard-pointer-<prefix>.txt (guest form
saw clicked=0 typed=0)`.

## Reading

This is the first run in which the guest itself reported what it received.
The form was ready, on top and under the click point on the active user
desktop, and the agent reported every injected event as inserted. Yet the
form saw neither a click nor a character.

So the agent's `SendInput` injection does not reach this window. The
earlier D5 input runs, which inject into a window launched the same way, never
passed either. Candidates include:
- User Interface Privilege Isolation silently dropping input into a window
  with a higher integrity level; `SendInput` still returns a full count.
- A session or desktop mismatch between the agent and the form.
- Another window taking the input at that moment.

r46 retained nothing that separates them. The final frame is black again,
because the guest shut down before the packet was taken, and the guest agent
log in the packet ends before the input.

## Follow-up

Both ends of the input now report where they run:
- The form's progress line adds its session, integrity level, whether it is
  the foreground window, and the cursor position. A moved cursor would show
  the injected pointer reached Windows.
- The launcher, which runs with the agent's token, prints the agent-side
  session and integrity level and the workload's session into the run log.

A journey failure after first READY now freezes the guest with the existing
host diagnostic stop before the app stops, so the packet keeps the screen as
it was at failure. None of this has run on hardware; A9 and A11 remain OPEN
and product state stays ENGINEERING_PREVIEW.
