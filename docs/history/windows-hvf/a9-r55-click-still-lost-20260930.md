# A9 r55 click still lost without renegotiation — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic, and a retraction
of the r54 reading. The current A9 criterion, defect wording and product state
come from `capabilities/windows-hvf.json`. A9 remains OPEN and the product
remains ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-04108f22-focus-click-pilot-r55` ran
`04108f22ea9ecebebba708f4261221dc104f343d`, the PR #289 head. That head keeps
the negotiated input stream across a host target change (see
[r54](a9-r54-focus-click-refused-20260930.md)). The app was a local
exact-source Apple Development build, and the Accessibility preflight and
console check passed. The input manifest SHA-256 was
`15f331141640faa06d9d338046078762f0da3a2011048d7a3774076e56cec095`.

The job ran on Mac17,9, macOS 27.0.1, from 00:41:54 to 00:54:48 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`aa2a21ecba6b0311fe8997ecaa4d7439c66a315e34291daaf36dc3fc43ce9888`) passed the
strict read check, the lane result SHA-256 was
`fe2dffb8b209ed6ff44f1025c885738f6870160ed024df37b458eed6b7d2b051`, and worker
cleanup was verified. All 26 half-minute samples taken while the job ran found
the console unlocked and the product app frontmost.

Installation, Secure Boot provisioning and first READY passed. The lane failed
again with `display click was not inserted as guest pointer input`. The final
frame again shows the Start menu open over the challenge form.

## Reading and retraction

The app negotiated guest input once, before the KeyboardPointer workload. The
second `INPUTCAPS` that r54 showed is gone, so the input-target fix did what it
says. The app still sent no `POINTERINPUT`. The r54 reading, that
renegotiation refused the click that focused the display, is therefore not
established. Renegotiation may have been one cause or none. Something else
keeps the click from reaching the ordered input stream.

The app's display view drops a press silently in two cases: when it has no
guest size, and when the router refuses the event. The app's own input
diagnostics go to its event list, not to the retained lane. The run log alone
cannot tell which of these happened, or whether the view received the press
at all. The same click reached the guest in r49, r50, r52 and r53.

## Same-run measurements

No criterion measures these. This is a second hardware run with the 40 ms
CoreAudio start reserve:

- The second probe process had 1 gap in 1,803 callbacks. That gap was the one
  wholly silent contention callback.
- The third had no gap in 336 callbacks.

The display export kept 30.3 polls a second and published 20.4 to 25.6 changed
frames a second.

## Follow-up

The next pilot needs the app's view of the click: whether the view received a
press, whether it had a guest size, and what the router returned. No further
run is useful until that is retained. A9 and A11 remain OPEN, and product state
stays ENGINEERING_PREVIEW.
