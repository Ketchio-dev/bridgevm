# A9 r50 audio counter gate — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-fddde07f-audio-directory-pilot-r50` ran
`fddde07fe5e9f93967d0cd7f53cae5d04f2ea5f5`, the PR #278 head carrying the guest
data directory and strict guest failure reporting that followed
[r49](a9-r49-input-passes-audio-directory-20260930.md). Its full project check
`36694863066` succeeded. The app was a local exact-source Apple Development
build, the Accessibility preflight and console check passed, and the input
manifest SHA-256 was
`f12768d69fd2b84502c5fe8fd41a7aadecc57811b345adbc3765dafb61f4c00c`.

The job ran on Mac17,9, macOS 27.0, from 09:17:05 to 09:30:54 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`713cb746c984315c5ff5820ee9ffc4ecaeeba2b01867129b1bb501bc23161eaf`) passed the
strict read check, the lane result SHA-256 was
`effe2b74d0a46c11c8d59838fd648b018a409074471932287afb2a8153e5906c`, and worker
cleanup was verified. All 29 half-minute samples found the console unlocked,
and the product app was frontmost in 27 of the 28 taken while the job ran.

Installation, Secure Boot provisioning and first READY passed, the thirteenth
consecutive first READY. The keyboard-and-pointer, clipboard, folder-share
and network stages passed again. This time the audio workload wrote its
output, the first snapshot marker was written, and the guest shut down
cleanly with the exact host-framed SYSTEM_OFF. T17 then failed its host audio
check: `guest audio lacked successful host CoreAudio counters`.

## Reading

The final host report's audio counters were `frames_rendered=206374`,
`drops=0`, `queue_stop_errors=0` and `queue_dispose_errors=0`, with
`callback_errors=3`. All three were typed as expected stopping statuses:
`callback_expected_stopping_errors=3`, `callback_unexpected_errors=0`, all
`stopping_enqueue_during_reset`. The r49 run log shows the same three
statuses at each stop.

T17's audio check, added on 2026-09-01, requires `callback_errors == 0`. B7,
proven the same day, established that every playback-and-shutdown produces
these expected stopping statuses; its statement requires zero *unexpected*
callback errors with expected statuses separately typed and counted, and its
ten-run receipt counted 30. A5 requires `frames_rendered>0` and `drops==0`.
So T17's audio check as written cannot pass once the guest has shut down, and
it is stricter than both proven audio criteria.

Changing that check after observing this failure would redefine a pass
threshold mid-investigation. Suppressing the statuses in the host would
change the behaviour B7's evidence measured. Both are left to the operator,
and the check is unchanged.

## Status

Nothing is promoted. A9 and A11 remain OPEN and product state stays
ENGINEERING_PREVIEW.
