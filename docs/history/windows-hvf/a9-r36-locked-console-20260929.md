# A9 r36 locked-console pilot — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-97328d9d-provisioner-fix-pilot-r36` ran exact main
`97328d9d2f381315fccd10be3efbdc03776ac5ee`, whose 50 main-push hosted workflows
succeeded, including CI `36643243401`, Security and quality `36643243427` and
the full project check `36643495902`. It was meant to be the first live run of
the [first-logon provisioner correction](a9-firstboot-provisioner-and-shutdown-gate-20260929.md).
The app was a local exact-source Apple Development build, the LaunchServices
Accessibility preflight passed, and the input manifest SHA-256 was
`3670512df367be8051f5a6e6087167e0ffe5848c98cf3ca3d04eac5ed5b2b01c`.

The job ran on Mac17,9, macOS 27.0, from 23:29:30 to 23:30:43 UTC and failed
with exit status 1. Public and private receipts were byte-identical at SHA-256
`9b97ca6ae7c28fff655cc9e21bd9bb07422d836827ebac613263c083a7a05df2`; both
official validators and `bridgevm-live receipt` passed. Artifact preflight
passed; VM creation did not. Worker cleanup was verified.

The authenticated lane result SHA-256 was
`231d409520811c58320062ab1cd2b06eb3e02a51340ade5a9858cd6aea05202a`. It
recorded `ui-element-missing` for the first-run create control within its
30-second bound. The bounded AX capture reported the app process present with
one window, readable focused and main windows and at least 127 nodes, but the
app neither active nor frontmost.

## Host state

After the run, the console session reported `CGSSessionScreenIsLocked=true`
with a lock time of 22:16:40 UTC, over an hour before the job started. An
unlock and re-lock in between would have replaced that time, so the screen was
locked for the whole run. r35, which ran before this lock began, passed the
same step. The pilot therefore did not exercise the product flow or the
provisioner correction, and says nothing about either. The submitting session
did not check the lock state; that was a submission error, not a product
result.

## Follow-up

T17 submission now requires this user's console session to be on screen,
logged in and unlocked. The check runs after the existing manifest and
Accessibility preflight and refuses before a job ID is used. A synthetic
contract covers the refusal states. On this host, with the screen still
locked, the real r36 manifest passed the preflight and was then refused. The
session can still lock after submission, and T19 import submission has no such
check yet.

## Limit

No product behaviour is observed here. The provisioner correction remains
untested live. A9 and A11 remain OPEN and product state stays
ENGINEERING_PREVIEW.
