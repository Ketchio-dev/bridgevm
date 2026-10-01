# A9 r54 focusing click refused — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`. A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-b0ee285a-restore-prefill-pilot-r54` ran
`b0ee285a12a6f49f97a5c6555cfc155662b5d187`, the PR #288 head. That head pools
each digest read and starts CoreAudio playback after a 40 ms reserve (see
[r53](a9-r53-restore-helper-lost-20260930.md)). The app was a local
exact-source Apple Development build, and the Accessibility preflight and
console check passed. The input manifest SHA-256 was
`5528242508bf7690af9d3371fb49a70a5c6a0c7c22d5c7c66f8e0319a8e5d88b`.

The job ran on Mac17,9, macOS 27.0.1, from 00:16:42 to 00:29:40 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`102c3979750fb74c5508e8958566eb7cb0626d834076b6602a660c06386fbb67`) passed the
strict read check, the lane result SHA-256 was
`bf95478bb35b4630ed3e02d2ec3764b8b47b380691716e181add4a4e39e94df4`, and worker
cleanup was verified. All 26 half-minute samples taken while the job ran found
the console unlocked and the product app frontmost.

Installation, Secure Boot provisioning and first READY passed. The lane then
failed with `display click was not inserted as guest pointer input`. The host
diagnostic stop completed, and a post-READY packet was retained. The restore
stage, which the digest fix targets, was not reached.

## Reading

The final frame again shows the Windows Start menu open over the challenge
form, as in [r48](a9-r48-start-menu-not-foreground-20260930.md). The run log
shows the cause on the host side:

- In r50 and r52, the app negotiated guest input once, before the KeyboardPointer
  workload. It sent `POINTERINPUT` right after the form reported ready.
- In r54, after the form reported ready, the app negotiated again (a second
  `INPUTCAPS`). It sent no `POINTERINPUT` at all.

The app's display view cancels the ordered-input target in three cases: when
the view moves into a window, when its window's focus changes, and when it
resigns first responder. That cancellation reset the negotiated stream, so the
app renegotiated input capabilities. While it renegotiated, a router that had
already been active refused input. So the click that focused the display was
dropped.

A real user who clicks an unfocused VM window can lose that first click the
same way. The app's own input diagnostics are not retained in the lane, so
this account comes from the log and the code, not from a recorded refusal.

## Same-run measurements

No criterion measures these. The 40 ms CoreAudio start reserve ran for the
first time on hardware. These are single runs, not a comparison at a gate's
sample count.

| Probe process | r52, no reserve | r54, 40 ms reserve |
| --- | --- | --- |
| second | 1,789 callbacks, 15 underruns, 13 gaps, largest 944 frames | 1,733 callbacks, 2 underruns, 2 gaps, largest 137 frames |
| third | 447 callbacks, 14 underruns, 14 gaps, largest 476 frames | 343 callbacks, 0 underruns, 0 gaps |

The display export kept 30.3 polls a second and published 20.3 to 25.6
changed frames a second.

## Follow-up

The negotiated stream now survives a host target change. The change cancels
pending requests and releases a held button as before. It does not renegotiate,
and it still never falls back to the legacy HID path. The router and
pointer-recovery tests now require this. The fix has not run on hardware. A9
and A11 remain OPEN, and product state stays ENGINEERING_PREVIEW.
