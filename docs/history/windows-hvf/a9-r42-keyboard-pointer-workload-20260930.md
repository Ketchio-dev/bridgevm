# A9 r42 keyboard and pointer workload — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-5c15ace6-send-field-fill-pilot-r42` ran
`5c15ace6cdb54f47ab0ca988a9a039efd129a9a9`, a PR #278 head carrying the
[r41](a9-r41-send-field-binding-20260930.md) send-field fill. It was submitted
before that head's hosted checks finished. The app was a local exact-source
Apple Development build, the Accessibility preflight and console check passed,
and the input manifest SHA-256 was
`23ae0d9eebaac617f95a9bc906e7cb974bdbb9f7b3f18dcfe399db1f4280cee8`.
Background-priority builds of unrelated branches ran on the host.

The job ran on Mac17,9, macOS 27.0, from 04:05:44 to 04:18:31 UTC and failed
with exit status 1. Byte-identical receipts (SHA-256
`3cdb190ea56312e43da32331e4c199b76acc5741f49a85f67831639971717ee0`) and the
lane result (SHA-256
`0fa47223db9a78ffd786988d66842fb8c601166115fc2d28aeebd0151a67f6b4`)
validated, and worker cleanup was verified. Half-minute samples found the
console unlocked throughout, with the product app frontmost except at the
start and end.

Installation, Secure Boot provisioning and first READY each passed once, the
fifth consecutive first READY. For the first time the lane sent a guest
control command and received its nonce-bound launch completion for the
keyboard-and-pointer workload. It then opened the display, clicked the display
window, filled and sent the keyboard challenge, and failed:
`guest workload did not produce t17-keyboard-pointer-<prefix>.txt` after the
60-second wait.

## Reading

No guest frame or log was retained for a failure after first READY, so the
cause is not established. The guest workload counts only clicks and
keystrokes that arrive after its WinForms form is shown, while T17 sends them
as soon as the launcher reports that the process was created; on a freshly
installed guest the input can arrive before the form exists. That race is a
design defect by static reading and is one hypothesis. Keyboard or pointer
injection failing, or the click missing the form, are others.

## Follow-up

T17 is gaining a readiness handshake, in which the guest form reports that it
is shown before any input is sent, and private diagnostic retention for
failures after first READY. Neither has run on hardware. No journey stage
after first READY passed; A9 and A11 remain OPEN and product state stays
ENGINEERING_PREVIEW.
