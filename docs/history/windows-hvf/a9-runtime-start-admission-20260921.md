# A9 runtime Start admission — 2026-09-21

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r21 result

Exact-main commit `df3f185e4c7b80ab64d7f0cd6b85b44f47b5eb8f` was
packaged as an Apple Development-signed, 3D-off artifact and used by physical
pilot `t17-df3f185e-committed-text-r21`.

The pilot passed artifact preflight, exact VM creation, source preparation,
Windows installation and Microsoft-only Secure Boot provisioning. It then
failed before first READY/PONG with `failure_code=guest-evidence-missing` and
`run_log=status=absent`. The strict public result remained
`first-boot-failed`, `first_ready_passes=0`, `criterion_pass=false` and
`claim_eligible=false`. Cleanup was verified and 3D injection remained false.

The strict receipt passed repository verification and has SHA-256
`2a89a5a413714ee1862331adf92f591a031a498208d6fe4da74f0596acb5bf44`.
The authenticated private lane result has SHA-256
`6a31c0297bc4044cf0297850c8d6f3250408db81d75e60df09924762d4f5819a`.
Private paths and guest material remain outside git.

## Measured UI state

During the first-ready wait, the app showed the exact T17 VM stopped. The Start
button was disabled and the readiness card reported `share-invalid`. The sealed
request's host-share directory existed, but no runner or run log was created.
The helper nevertheless entered the 600-second first-ready wait because macOS
returned success for `AXPress` on the disabled control. An AX action success was
therefore not proof that the product accepted Start.

The host-share accessibility identifier was also attached to the complete path
row. Earlier snapshots showed that identifier on the text field, label and
folder button. Exact role qualification avoided selecting the wrong role, but
the field did not own a unique product identifier.

## Correction

Source `522d97ce` moves the host-share identifier onto its editable text field.
The product helper now reads the target's supported `AXEnabled` value and waits
to a fixed deadline before pressing. A missing enabled value or attribute read
failure propagates, a control that remains disabled fails without an AXPress,
and activation retry remains available only after an enabled control rejects
the first press.

Seven focused contracts cover enabled and disabled actions, missing and failed
enabled reads, activation retry and bounded diagnostics. The isolated shim rerun
passed 856 BridgeVMControl, 62 AppleVzRunnerCore and 130 ProductE2E tests; log
SHA-256 is `b5fb35c2d1153e6f08a75d0a60256c71de2230047791dda3d441ee2631a74a3e`.
The fast project check passed at SHA-256 `9eb40dace39192a0361bbafd0c3d98bec71abe9b0f650ae7e26eaca50e1794c6`.
Exact metadata-head hosted verification and a new signed pilot remain required.

## Limits

This change prevents a false action acknowledgement and removes ambiguous
host-share identity. It does not prove that the SwiftUI binding becomes launch
ready, that the VM starts, or that Windows reaches READY/PONG. A new physical
pilot must distinguish an early truthful disabled-control failure from a real
accepted start. One successful pilot remains a live single run rather than the
required release sample count, and T19 remains required. No criterion,
threshold or timeout changed.
