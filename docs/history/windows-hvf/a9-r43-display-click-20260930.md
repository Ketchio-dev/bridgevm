# A9 r43 display click — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-60de6564-ready-handshake-pilot-r43` ran
`60de6564604b4de67b5f05faaf8cc171cddd8bd2`, a PR #278 head carrying the input
challenge readiness handshake and post-READY private packet that followed
[r42](a9-r42-keyboard-pointer-workload-20260930.md). It was submitted at
05:31:03 UTC, before that head's hosted checks finished; all 95 hosted runs,
including full project check `36673586391`, later succeeded. The app was a
local exact-source Apple Development build, the Accessibility preflight and
console check passed, and the input manifest SHA-256 was
`eab7653a151574dc520314b0dbfd04ad2649262200223d02b57fe4e1208f20f0`.

The job ran on Mac17,9, macOS 27.0, from 05:31:28 to 05:44:03 UTC and failed
with exit status 1. Byte-identical receipts (SHA-256
`0965fdbc020924c1f9e4233eaab227f1bdb8c6142a9feb3458ba6bb61f99ba39`) passed the
strict read check, the lane result SHA-256 was
`ebf9e80ba56b0437a30dadb101ce122dc67acc94eb0e5c1b195d40131d41a085`, and worker
cleanup was verified. All 27 half-minute samples found the console unlocked;
the product app was frontmost in 25.

Installation, Secure Boot provisioning and first READY each passed once, the
sixth consecutive first READY. The keyboard-and-pointer workload launched, and
the guest wrote its nonce-bound ready marker. The form was therefore shown
before any host input. T17 then opened the display, clicked it, and filled and
sent the keyboard challenge. It failed with
`guest workload did not produce t17-keyboard-pointer-<prefix>.txt`.

## Reading

The retained private packet settles part of the question. The run log carries
one ordered input request, `TEXTINPUT`, which the guest agent inserted as
36 events, and no `POINTERINPUT` request at all. The share holds the ready
marker and no output. Keyboard text and display clicks share one ordered agent
stream. A click routed to the legacy path would have moved later text to it,
and each refusal that could apply with nothing queued or held also refuses
later text. So the display view never turned the host click into a guest
pointer request. The handshake held input until the form was shown, so the
r42 race did not apply here; r42 itself remains unexplained.

The display view drops a press until it has presented a guest frame. T17 found
the display window and clicked its centre as soon as the window existed. A
local probe of the same window construction found the posted click reached the
view as a key-window mouse-down 30 to 60 ms after the window opened. So a
first frame later than that would drop the click silently. That is the leading
explanation, not an established one, because r43 retained no app-side record
of the press.

## Follow-up

The display surface now reports through accessibility whether a guest frame is
presented. T17 clicks the surface only once it reports a frame in the focused
window. It then requires two inserted `POINTERINPUT` receipts, the press and
the release, before it types, and fails with a specific blocker otherwise.
None of this has run on hardware. No journey stage after first READY passed;
A9 and A11 remain OPEN and product state stays ENGINEERING_PREVIEW.
