# A9 r51 host focus lost — invalid physical diagnostic

Classification: one invalid, nonpromoting physical diagnostic. The current A9
criterion and product state come from `capabilities/windows-hvf.json`; A9
remains OPEN and the product remains ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-0935b47a-b7-audio-pilot-r51` ran
`0935b47a24ef2f98a73d96ad3e219bb7bcc9e49b`, the PR #278 head carrying the
[B7-aligned audio check](t17-audio-check-b7-alignment-20260930.md). The app was
a local exact-source Apple Development build, the Accessibility preflight and
console check passed, and the input manifest SHA-256 was
`01e771ed97209fb7ccce71cd8712a6c2114e9dc3e41e6e6401a71052f67d8b6c`.

The job ran on Mac17,9, macOS 27.0, from 14:18:31 to 14:37:03 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`1d0795906f1c31679410d16f035dbfddaf426bde6b3ff2824c1ef02022f3da4b`) passed the
strict read check, the lane result SHA-256 was
`64ae1f1b51d63ead6aaa489ce0118b45c5aa559e4354726483d9b46ed97e7f61`, and worker
cleanup was verified. Installation, Secure Boot provisioning and first READY
passed, and the challenge form reported ready at the desktop switch. The lane
then failed with `display click was not inserted as guest pointer input`.

## Reading

The console stayed unlocked, but the product app was frontmost in only 14 of
the 39 half-minute samples. Other host applications held the foreground in
the rest: Aside in 18, System Settings in 6 and Reminders in 1, including
around the display click. T17 drives the product with real host clicks and
Accessibility focus, so a host session in use invalidates the input stages,
as in r39. r51 therefore says nothing about the product, the display click or
the new audio check.

## Same-session observation

While watching r51, the operator reported that the 3D-off display was very
choppy during boot and that guest audio stuttered badly. Neither is measured
by any current criterion or check; they are recorded as open work, not as
evidence from this job.
