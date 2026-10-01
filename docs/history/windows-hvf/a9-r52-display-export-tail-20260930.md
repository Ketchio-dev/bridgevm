# A9 r52 display export record after the footer — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`. A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-fc987522-display-audio-pilot-r52` ran
`fc9875227d58753b1bc13574a1daa4a7b21fd81a`. That is the PR #278 head, which
carries the dedicated ramfb display export thread and the CoreAudio continuity
record, and it is an ancestor of main. The app was a local exact-source Apple
Development build, and the Accessibility preflight and console check passed.
The input manifest SHA-256 was
`790d67f96e2a424938636b17a6967de52e33fd5d5918edbd40517db6ec5a7688`.

The job ran on Mac17,9, macOS 27.0.1, from 23:13:55 to 23:32:10 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`4b4c48eeab2b97992dc55af939c555afb85e6d1d91dc085a90c2d9f2891dfc92`) passed the
strict read check, the lane result SHA-256 was
`ad1bad7536602cd801ab63ebeea77c9710966ad31c260980e0c0af9938e0ab5f`, and worker
cleanup was verified. All 36 half-minute samples taken while the job ran found
the console unlocked. The product app was frontmost in 35 of them; the other
sample, six seconds before the job finished, showed a terminal application.

Installation, Secure Boot provisioning and first READY passed, followed by
keyboard and pointer, clipboard, folder share and network, as in
[r49](a9-r49-input-passes-audio-directory-20260930.md). The audio workload wrote its
output. The first shutdown then failed with `guest did not reach clean
SYSTEM_OFF`, so the audio, snapshot and second-boot stages were not reached.

## Reading

The retained run log shows that the guest did power off:
`stop: PSCI 0x84000008 (system off)`. After the report's footer, the log holds
`ramfb display export: stopped ...` and then the CoreAudio records. The export
thread printed that record when it was dropped at the end of the probe. Every
reader of the post-footer host tail admits only audio records there. On the
recorded log, the Python twin of the terminal-report grammar returns no stop.
With only that record removed, the same grammar returns the system-off stop.
The failure is therefore the probe's print order, not the guest shutdown.

The final CoreAudio stats record would have passed the B7-aligned audio check
(frames 211,022, no drops, three expected stopping errors and no unexpected
error). The journey never judged it, because the shutdown check failed first.

## Same-run measurements

No criterion measures these; they are recorded for the operator's report of a
choppy display and stuttering audio. The retained log covers three probe
processes:

| Process | Seconds | Export polls/s | Frames published/s | Slowest tick | Audio callbacks | Underrun callbacks | Gaps | Largest gap |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 (reset) | 100.6 | 30.3 | 21.9 | 758 µs | 0 | 0 | 0 | 0 frames |
| 2 (reset) | 182.4 | 30.3 | 26.6 | 974 µs | 1,789 | 15 | 13 | 944 frames |
| 3 (system off) | 163.7 | 30.3 | 19.6 | 937 µs | 447 | 14 | 14 | 476 frames |

The export kept its 33 ms cadence and published a frame whenever the guest
had changed it. Whether the app's window now looks smooth was not measured.
Audio still underran in about 1–3% of callbacks, with gaps of up to 20 ms, so
the stutter the operator heard is not resolved.

## Follow-up

The probe now drops the export thread on the terminal path, before the final
report, and the readers' grammar is unchanged. A new order contract failed on
the previous source and passes on the fix. None of this has run on hardware;
A9 and A11 remain OPEN, and product state stays ENGINEERING_PREVIEW.
