# CoreAudio PCM fragment boundaries — 2026-10-08

Evidence: deterministic PCM-byte tests and static review, not live playback or
listening. No claim that this defect caused the observed Windows audio stutter.

The HDA poll budget is frame-aligned, but descriptor boundaries can split it.
A real-controller test accepts descriptors of six and two bytes for stereo-s16
and delivers two sink writes. Previously a callback between those writes could
consume six bytes and insert two zeros, shifting the next stereo frame.
The primed callback regression failed with `[1,2,3,4,5,6,0,0]` rather than one
complete frame followed by silence. The failure is retained, not a live result.

## Repair and boundaries

The producer now keeps at most three bytes privately and publishes only complete
four-byte stereo frames. Ring-capacity rejection consumes whole completed
batches while retaining the final source fragment, so loss cannot shift phase.
One rejected publication batch counts one ring-full event; dropped bytes are
real source bytes, including any previously retained bytes completed by it.
Complete guest frames actually enqueued increment `frames_rendered`.

RUN pause and DMA fault retain carry because the device resumes its DMA cursor.
A separate default sink notification marks explicit controller/stream reset and
effective accepted format change, even after an error cleared RUN. It neither
changes guest-stop counts nor rejects descriptors or alters guest DMA semantics.

At that terminal boundary, a partial frame is zero-padded and cleared once.
This constructed frame preserves supplied bytes but is not a complete guest
frame and does not increment `frames_rendered`. A rejected terminal frame counts
only its one to three real source bytes, not padding. Reset marks playback idle
without inventing a guest-stop event, allowing existing short-sound flushing.
Unsupported format and sink destruction also finalize carry; immediate queue
stop does not prove the terminal frame was played. Existing stats schema and
exact drop reconciliation are unchanged. No buffering threshold was raised.

## Checks and retained failures

- Original regression: one failure/exit101; actual DMA6+2 companion: one pass.
- Reintroducing direct byte publication after repair reproduces the same failure;
  mutation removed before final tests.
- CoreAudio focused39/0, library1358/0 plus one existing ignore, example520/0;
  Clippy library/example with warnings denied passed.
- Real HDA→producer→callback tests interleave callbacks between DMA fragments
  and cover six-byte DMA/error/controller reset without creating an AudioQueue.
- Cases include every split of64bytes, carry lengths1–3, pause/resume, repeated
  reset, format rejection, capacity loss, exact counters and generation separation.
- Initial extraction missed a test-only `Shared` import; a later cross-module
  test used a non-public RUN constant. Both compiler failures retained and fixed.
  Worker patch module-registration overlap was refused, then combined explicitly.

A11 remains OPEN pending final release proof; A9/A19/product state unchanged.
Exact sealed local full and hosted checks remain required before integration.
