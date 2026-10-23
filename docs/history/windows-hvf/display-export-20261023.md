# Display-synchronised export and a hardened display reader (F1) — 2026-10-23

Classification: an app and VMM host-side change for the 3D-off display stutter
(A9 `known_defect`), fix F1 of the 2026-10-22 deterministic investigation. The
evidence is automated tests (rank 3), one host bench (rank 3) and static
reasoning (rank 4, marked **[4]**). **No live run exercised any of it, and
nothing here observed the screen or a guest.** A9 stays OPEN with its
`known_defect`, and the product state does not change. The guest cannot see
any of it, so there is no machine-contract deviation entry.

Logs, the bench source and its data stay outside the repository, in the
operator's evidence directory `dev-20261021-evidence/display-export`
(`logs/`, `bench/`).

## Commits

| Commit | What it establishes |
|---|---|
| `25b0bc83` | The display reader is one seqlock reader, `HvfFramebufferFrameReader`, with acquire ordering and injectable clock, sequence load and pause. Its retry knob is at the old value, 0, so behaviour is unchanged; every existing shim test passes on it. |
| `4934d2d3` | A tick retries an odd or moved sequence up to 2 times within 0.5 ms. |
| `8e17a02d` | The ramfb exporter answers a frame request in the display file's header within 1 ms, at most one frame per request, and keeps its 33 ms deadline as the fallback when nobody asks. |
| `ac41a48f` | The display tick posts one frame request per tick to a writer that answers requests, and writes nothing to any other writer's file. |

## Part A: the reader

Paths are under `apps/macos/Sources/BridgeVMControl/HvfEngine/`.

- `HvfFramebufferView.refreshFrame` keeps the tick (sidecar check, mapping,
  image, layer) and asks `HvfFramebufferFrameReader.read` for the frame. The
  reader returns a frame, unchanged, busy or invalid; the view treats each as
  before (busy and unchanged keep the shown frame, invalid resets the mapping
  and the display).
- **Acquire ordering.** The sequence at byte 24 is loaded, then an acquire
  fence; another acquire fence precedes the re-check after the copy
  (`HvfFramebufferScalarReads.swift`, `HvfFramebufferFrameReader.swift`).
  Before, both were plain loads with no fence, which is not a correct seqlock
  reader on ARM64.
- **Retry.** After an odd sequence or a re-check that does not match, the tick
  tries again while it has made fewer than 2 retries and less than 0.5 ms has
  passed since its first read. Between attempts it spins on the sequence until
  the writer has left the frame or the budget is spent. When either limit is
  reached it keeps the shown frame, as before.

Decisions:

1. **The knob first, at its old value.** `25b0bc83` extracts the reader with
   `retries = 0`, and `4934d2d3` sets it to 2 with the tests. The failing-first
   tests therefore ran against a committed state, not a scratch edit. Because
   new files are registered at their size and never raised, the extracted
   reader already had its final shape in `25b0bc83`.
2. **The fence.** It is `atomic_thread_fence(memory_order_acquire)` from the
   clang `_Builtin_stdatomic` module, which links to libSystem's
   `_atomic_thread_fence` and builds for the macOS 14 deployment target. The
   Synchronization module's `atomicMemoryFence` needs macOS 15, and
   `OSMemoryBarrier` is deprecated.
3. **Spin, not sleep, between attempts.** Every timed wait on this host wakes
   milliseconds late (Step 2 below), and `sched_yield` can give the core away
   for a scheduler quantum; either would break the 0.5 ms bound.
4. **A writer that dies mid-frame.** The sequence then stays odd, and every
   tick spends the whole 0.5 ms spinning on the main thread: 3 % of it at 60 Hz
   and 6 % at 120 Hz, until the view goes away. Accepted: the VMM is gone in
   that state. **[4]**
5. **Double buffering (A2) is not done.** It is not trivial. Two pixel slots
   and a slot index change the BVFB layout that `fb_sink_write.rs` shares with
   the virtio-gpu device (a file at its line and unsafe ceilings), that
   `display_fb.rs`, the Swift reader, `scripts/fb-rate.py` and
   `scripts/live-gates/t17_display_header.py` read, and that check
   `version == 1` and a file length of 64 + height × stride. It would also
   double `display.fb`. With the pulled export the writer publishes within a
   millisecond of a tick and about one refresh before the next read, so a read
   rarely meets a frame mid-write. **[4]**

## Step 2: the timer bench that decided B1 or B2

The investigation never ran its timer test at `QOS_CLASS_USER_INTERACTIVE`.
This bench did, before Part B was chosen.

- **What ran.** The exporter's real wait, `StopSignal::wait_until` in
  `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/ramfb_display_stop.rs`
  (std `Condvar::wait_timeout`, which on this toolchain imports only
  `pthread_cond_timedwait_relative_np`), paced as `export_loop` paced it. It
  ran on a thread named like the exporter's, with no frame work between waits,
  from a scratch test hooked into a detached scratch worktree and never
  committed (`bench/scratch_qos_wake_bench.rs`).
- **QoS.** The default thread reads back as `QOS_CLASS_DEFAULT` and ran at
  priority 31. The elevated thread called
  `pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0)`, which
  returned 0 and reads back as user-interactive, but `ps -M` showed it at
  priority **37**, not 47: this process's request is clamped to the
  user-initiated band.
- **Runs.** 2100 wakes per run, two rounds, nice 0 (wake lateness depends on
  priority, and the product VMM is not niced). Mac16,9, macOS 26.7; the
  1-minute load average was 2.3–5.8.
- **Rule, fixed before the run.** B1 only if every user-interactive run had at
  least 95 % of its wakes within 1 ms at both periods.

| Round | Period | QoS | p50 ms | p90 ms | p99 ms | max ms | ≤ 1 ms |
|---|---|---|---|---|---|---|---|
| 1 | 8.333 | default | 2.780 | 3.257 | 4.036 | 6.842 | 8.67 % |
| 1 | 8.333 | user-interactive | 2.424 | 3.506 | 4.121 | 5.412 | 18.00 % |
| 1 | 16.667 | default | 5.400 | 6.467 | 7.979 | 17.003 | 8.10 % |
| 1 | 16.667 | user-interactive | 5.355 | 6.531 | 8.053 | 8.334 | 8.14 % |
| 2 | 8.333 | default | 2.598 | 3.525 | 4.132 | 8.244 | 16.10 % |
| 2 | 8.333 | user-interactive | 2.764 | 3.390 | 4.067 | 4.173 | 11.86 % |
| 2 | 16.667 | default | 4.833 | 6.944 | 9.308 | 37.603 | 8.24 % |
| 2 | 16.667 | user-interactive | 5.368 | 7.272 | 8.261 | 8.340 | 8.57 % |

**Decision: B2.** At user-interactive QoS 11.9–18.0 % of wakes were within
1 ms at 8.333 ms and 8.1–8.6 % at 16.667 ms. Raising the QoS barely moves the
median, about a third of the period either way; it caps the tail at half the
period, where the default run reached 17.0 and 37.6 ms. The live T17 r92
export, at its 33 ms period, had 6.4 % of wakes within 1 ms: the same regime.
A timer-driven export at the display rate would therefore jitter by several
milliseconds per frame on this host. **[4]**

## Part B: the pulled export (B2)

### The header, still version 1

`crates/bridgevm-hvf/src/display_fb_requests.rs` defines two words in the
header's reserved bytes:

- **Bytes 32..40, the request.** The reader writes it with `pwrite`, once per
  display tick. It is the reader's `CLOCK_UPTIME_RAW` time in nanoseconds, so
  the writer can tell both that a new request came and when it was posted.
- **Bytes 40..44, the marker.** "BREQ", set only by a writer that answers
  requests. A reader posts nothing until it sees it.

The writer never writes these bytes: `FbSink` writes only the 24 header bytes,
the sequence and the pixels. Compatibility, each case with the test that pins
it:

| Pairing | What happens | Test |
|---|---|---|
| Older app, new VMM | No request ever comes, so the exporter exports on its 33 ms grid as before, and the reader ignores bytes 32..64. | `an_app_that_never_asks_still_gets_frames_on_the_deadline`, and the existing timed exporter tests, which now run through the new wait |
| New app, older VMM | No marker, so the app writes nothing and still takes every published frame. | `testDisplayTickLeavesAWriterWithoutTheMarkAlone` |
| 3D on (virtio-gpu sink) | The device's sink neither sets the marker nor touches the words, through `write` or `write_damage`. The ramfb exporter stays off while the device exists. | `the_virtio_gpu_sink_neither_marks_nor_overwrites_the_request_words`, the existing `inert_for_the_process_when_the_machine_has_a_virtio_gpu_device` |
| A marker left in a reused file | The reader posts requests that nobody reads: one `pwrite` per tick, nothing else. | not tested **[4]** |
| Other readers | `scripts/live-gates/t17_display_header.py` and `scripts/fb-rate.py` read only bytes 0..32 and still require version 1. | static **[4]** |

### The exporter

- **The wait.** The export thread waits on one kqueue
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/ramfb_display_wake.rs`)
  for its stop (a user event), a vnode write to `display.fb`, or the fallback
  deadline. This follows the existing `input.ctl` wake
  (`input_control_wake/vnode.rs`). A request wakes the thread as an event, not
  as a timer, so the coalescing measured above does not apply to it. Without a
  kqueue the wait is the old timed one.
- **Watch before marking.** After its first published frame the thread opens
  the file, watches it and only then sets the marker. The app asks only once
  it will be heard, so no request is lost.
- **One tick per request.** A write whose request word differs from the last
  one seen is one tick: copy, compare, publish if changed. Several requests
  before the thread looks again are one tick, so it publishes at most one frame
  per request. Writes that leave the word unchanged are ignored.
- **The deadline is the fallback.** A request tick re-arms the 33 ms deadline
  a whole period ahead, so while an app asks at its refresh rate the deadline
  never fires. Without requests (no app, an older app, a hidden or minimised
  window whose display link stops, an unwatchable file) the thread exports on
  its deadline grid as before. The period stays 33 ms
  (`crates/bridgevm-hvf-runtime/src/vm_process_surfaces.rs`), and the 16 ms
  floor of `BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS` is unchanged.
- **run.log.** One new line, `ramfb display export: answering frame requests`
  or `... frame requests unanswered, deadline only`, printed once at the first
  frame. It is not a host record and nothing parses it.

### The app

`HvfFramebufferFrameRequester.swift` posts the request from `refreshFrame`
after the tick's read, so the exporter's copy never races that read. It writes
through its own `O_WRONLY | O_NOFOLLOW | O_NONBLOCK` descriptor, checked
against the same path and identity as the mapping, and only when the mapped
header has the BVFB magic and the marker. The descriptor is closed whenever
the mapping is reset. Making room for this in `HvfFramebufferView.swift` moved
`ensureMapping` and `resetMapping` into `HvfFramebufferViewMapping.swift`
(321 → 304 lines).

### Cost

While the window is visible the exporter now copies and compares once per
display refresh instead of 30.3 times a second. From the investigation's
per-tick cost (T3 p50) **[4]**:

| Frame | 60 Hz display | 120 Hz display | Before (33 ms) |
|---|---|---|---|
| 800x600 (the 3D-off geometry) | 0.3–0.4 % of one core | 0.6–0.85 % | 0.15–0.21 % |
| 1080p | about 1.3–1.9 % | 2.6–3.7 % | — |
| 4K | about 5.5–8 % | 11–16 % | — |

The 60 Hz figures for 1080p and 4K are half of the measured 120/s figures.
The app adds one `pwrite` per tick on the main thread. vCPU exits do not
change: the exporter reads guest RAM through the host mapping and takes the
platform lock only with `try_lock` for the 28-byte config.

## Tests

| Test | File | Red (before) | Green |
|---|---|---|---|
| `testOddSequenceThatCompletesWithinTheTickIsTakenThisTick` | `HvfFramebufferSeqlockRetryTests.swift` | on `25b0bc83`: 0 frames made, expected 1 | `4934d2d3` |
| `testMismatchRetriesWithinBudgetThenGivesUp` | same | on `25b0bc83`: `[2, 0, 0]` loads/pauses/frames, expected `[6, 2, 0]` | `4934d2d3` |
| `publishes_within_one_ms_of_a_frame_request` | `ramfb_display_request_tests.rs` | on `4934d2d3`: timed out waiting for the writer to answer requests | `8e17a02d` |
| `publishes_at_most_one_frame_per_request` | same | same | `8e17a02d` |
| `the_deadline_exports_only_while_no_request_comes` | same | same | `8e17a02d` |
| `an_app_that_never_asks_still_gets_frames_on_the_deadline` | same | not run red (compatibility pin) | `8e17a02d` |
| three header tests, including the virtio-gpu sink | `crates/bridgevm-hvf/src/display_fb_requests_tests.rs` | not run red (new API, compatibility pins) | `8e17a02d` |
| `testEachDisplayTickPostsAFrameRequestToAWriterThatAnswersThem` | `HvfFramebufferFrameRequestTests.swift` | on `4934d2d3`: request word 0 | `ac41a48f` |
| `testDisplayTickLeavesAWriterWithoutTheMarkAlone` | same | passed on `4934d2d3` (a guard) | `ac41a48f` |

Notes:

- The Rust red runs failed at their first wait, for the marker: the old writer
  neither marks the file nor answers, so their request phase was never
  reached on `4934d2d3`.
- `publishes_within_one_ms_of_a_frame_request` asserts that the median of 20
  request-to-publish times, measured by the test, is at most 1 ms. It also
  asserts that the record counts 21 ticks and 21 frames, at least 11 of them
  within 1 ms. Every request must be answered within 1 s.
- Red and green logs are in `logs/`. A first red run of the retry tests,
  which failed to compile, is kept as
  `step1-red-shim-25b0bc83-plus-tests.run1-SUPERSEDED-test-compile-error.log`.

## The `ramfb display timing:` record after the change

The grammar and every field name are unchanged, as is where it prints. The
meanings change as follows:

- **`ticks`** is one per answered request plus one per deadline tick. With the
  window visible it follows the display's refresh rate instead of 30.3 a
  second, so `ticks × 33 ms` no longer estimates the run length; use the
  CoreAudio callbacks instead.
- **`late_*` and `max_late_us`**, for a request tick, are the time from the
  app stamping its request to the thread waking. For a deadline tick they are
  the time from the deadline, as before. The two kinds share the buckets.
- **`interval_*`** is still the time between publishes. While the guest
  changes every frame and the window is visible, it is one display period:
  16.7 ms at 60 Hz or 8.3 ms at 120 Hz, both in `interval_le_20ms`.

## Live confirmation on the next T17 first run

The r92 first-run record is the baseline: `ticks=5837` over about 193 s
(30.2 a second), `late_le_1ms=373` (6.4 %), `interval_le_20ms=3` and
`interval_le_40ms=2830` (81.1 % of intervals).

With the display window visible, the next first-run record must show:

1. `ramfb display export: answering frame requests` in the run log.
2. Ticks per second near the display's refresh rate: at least 50 a second on a
   60 Hz display, using the CoreAudio callbacks (10 ms each) for the duration.
3. `late_le_1ms` at least 90 % of `ticks`.
4. `interval_le_20ms` as the largest interval bucket, where r92 had
   `interval_le_40ms`.

Ticks near 30 a second with `late_le_1ms` near 6 % would mean that requests
were not answered: a missing marker, the `unanswered` line, or no display
tick. A window that was hidden for part of the run lowers items 2 and 3 in
proportion. The on-screen half still needs the app-side counters (frames taken
against published, early returns by reason, display-link intervals) and a
timed capture of the window; neither exists yet.

## Not verified

- **Nothing observed the screen,** and no live run or live-queue job ran.
  Whether the window looks smooth is unknown.
- The live VMM's timer and QoS regime, and whether an app-launched VMM's
  user-interactive request would be clamped as it was here. The bench ran in a
  process started under a launchd daemon on Mac16,9 with macOS 26.7; the live
  runs use Mac17,9 with macOS 27.0.
- The request wake latency across processes (app to VMM), and on a loaded host
  beyond what the probe suite's own parallel run exercised. The 1 ms tests run
  both ends in one process.
- That the exporter's own mapped stores and `set_len` never raise a vnode write
  event. If they did, the thread would wake, find the word unchanged and wait
  again. **[4]**
- A `pwrite` of the 8-byte word racing the exporter's read could tear it. That
  would give one extra tick and one wrong lateness. **[4]**
- The CPU figures at 1080p and 4K, which are scaled from the investigation's
  per-tick cost, not measured here.
- The main thread's cost of the retry spin, and of one `pwrite` per tick, under
  a real display link.
- Guest content rate, DWM cadence and tearing.

## Checks run

All from the worktree or a detached scratch worktree, with the 1.97.0
toolchain; logs in `logs/`.

| Check | Result |
|---|---|
| Probe example tests (`hvf_gic_boot_probe`, locked) | 708 passed on 63ad6ce5; 712 passed on ac41a48f |
| HVF lib tests (locked) | 1483 passed, 2 ignored on 63ad6ce5; 1486 passed, 2 ignored on ac41a48f |
| HVF lib tests with the venus feature | 1510 passed, 2 ignored on ac41a48f; not run on the base |
| cargo fmt over the workspace | clean |
| rustfmt, edition 2021, per changed probe file | `ramfb_display_thread_tests.rs` 8 hunks on the base and on the head; every other changed or new probe file 0 |
| clippy, workspace and all targets, warnings denied | pass, 0 warnings |
| clippy, bridgevm-hvf with the venus feature | pass, 0 warnings |
| `scripts/run-xctest-shim-suites.sh` (the whole runner) | all four suites pass on 4934d2d3 and on the ac41a48f tree: BridgeVMApp 430, BridgeVMControl 1467 then 1469 passed with 3 environment-gated skips, AppleVzRunnerCore 65, BridgeVMProductE2E 438 |
| `scripts/check-app-ui-host-contracts.sh` with a new scratch path | PASS |
| `scripts/build-production-input-driver.sh`, then `tests/integration/production-input-driver-test.py` | OK |
| `scripts/check-input-receipt-transcript.sh` | PASS |
| `tests/integration/t17-display-click-contract.py` | OK, 4 tests |
| `scripts/check-python-scripts.py` | PASS, 640 scripts |
| `scripts/check-refactor-budgets.sh` on each commit from 25b0bc83 to ac41a48f | PASS; the raise check over 63ad6ce5.. reports only lowered and new rows |
| `scripts/check-doc-references.py`, `scripts/check-documentation-system.sh` | PASS |

`scripts/check-project.sh` was not run, by instruction. The hand-listed
swiftc source sets in `scripts`, `tests`, `.github` and `apps/macos/scripts`
were searched; none lists the files or types this change adds or moves.
