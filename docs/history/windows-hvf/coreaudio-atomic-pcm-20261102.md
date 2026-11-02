# CoreAudio PCM publication without a producer mutex (2026-11-02)

The producer previously held the PCM ring mutex while copying a batch.
The realtime callback used try_lock and returned a whole silent buffer
on contention, even when previously published PCM was still available.

The transport now uses fixed AtomicU32 stereo-frame slots. A serialized
producer copies into free slots before one release publication; the
consumer captures that head with acquire and releases consumed slots only
after copying. Physical power-of-two indexing handles cursor wrap, while
the unchanged logical capacity still bounds accepted PCM to500ms.

A nonwaiting consumer guard preserves the callback overlap refusal path.
The producer does not hold that guard, so preparing the next batch cannot
block playback of already published frames. No unsafe shared storage,
new audio worker, timer, trim policy or resampling path is added.

The40ms prefill, complete-frame/terminal-fragment behavior, whole-batch
capacity refusal, source-byte drop accounting and record grammar remain.
The existing sink and AudioQueue context teardown ownership are unchanged.

A focused47-test run covers framing/flush/drop, prefill, HDA DMA, capped-gap
A5, continuity/timing/teardown and three atomic transport regressions.
The latter cover capacity, wrapping and the real callback draining old
PCM while a producer holds an unpublished batch. Subsequent extraction
and test-only annotations still require the final integration gates.

No listening check or live guest run was made. Removing this contention
path does not establish the cause of other long platform-lock gaps or
prove the disclosed audio defect resolved. No criterion or state promotion.
