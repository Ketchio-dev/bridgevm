# A9 r41 send-field binding — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-96561e3a-disclosure-expand-pilot-r41` ran
`96561e3a271e28e6cfe7f84fe22397ab8a09acdd`, the head of PR #278 with the
[r40](a9-r39-r40-diagnostics-opener-20260930.md) disclosure-triangle expand.
It was submitted before that head's hosted checks finished; CI `36665419328`
and Security and quality `36665419240` later succeeded. The app was a local
exact-source Apple Development build, the Accessibility preflight and console
check passed, and the input manifest SHA-256 was
`f1ef1bc90bced67a0fc1641125ca7e8d3560865f8c2f213b7edfa7d95a9aa5a6`.
Background-priority builds of unrelated branches ran on the host.

The job ran on Mac17,9, macOS 27.0, from 03:42:29 to 03:55:30 UTC and failed
with exit status 1. Byte-identical receipts (SHA-256
`827d407095a366b541c49ef5a3776e198abef03c2922bf970bda625e7cd8515a`) and the
lane result (SHA-256
`9ef95bb7a94e3d643e77c714dbbbb3743c8f9c8a700a51b8893bbfe252f606dd`)
validated, and worker cleanup was verified. Half-minute samples found the
console unlocked throughout, with the product app frontmost for most of the
run.

Installation, Secure Boot provisioning and first READY each passed once, the
fourth consecutive first READY since the provisioner correction. The lane
expanded the collapsed diagnostics, found the guest control input and entered
the command, then failed: `identified UI element remained disabled:
bridgevm.runtime.ctl.send`.

## Reading and correction

A cross-process probe rendered the same SwiftUI shape (a text field bound to
state, a Send button disabled while it is empty, and a commit action) and drove
it from a separate process as T17 does. Setting the value through `AXValue`
without focus displayed the text but left the binding empty, so Send stayed
disabled, and `AXConfirm` then fired the field's commit with empty text.
Focusing the field first updated the binding and enabled Send, and pressing
Send submitted the text exactly once. The app rejects the later empty commit
that fires when focus moves on, so nothing extra reaches the guest.

The guest control input and the keyboard input both submit through their own
action, so T17 now fills them by focusing, setting and verifying the value
without confirming. Other text fields keep the existing entry that passed live.
Unit tests pin the fill order and failures, and a contract that fails on the
previous tree pins which fields use it.

## Limit

This is one run; no journey stage after first READY passed, and the fill
correction has not run on hardware. A9 and A11 remain OPEN and product state
stays ENGINEERING_PREVIEW.
