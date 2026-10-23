# HDA and CoreAudio diagnostic records — 2026-10-22

Classification: report-only host diagnostics on the HVF probe's emulated
HDA → CoreAudio path. The evidence is automated tests and static reasoning.
**No live run has produced these records**, so nothing here says how a real
guest's polls, buffers or callbacks behave. No gate reads the records, and no
criterion, product state or capability wording changes. B7 was PROVEN at seal
`350a7e55` (re-proven at `13fb8213`, [B7 evidence](../../windows-arm/evidence/b7-audio-teardown-live-20260901.md)).
Its ten-run campaign was not re-run for this change. Instead, this change
leaves every B7 evidence file untouched and tests that the B7 scripts' output
does not change.

## Why

A read-only audio investigation on 2026-10-22 drove the real
`HdaController::poll` and the real CoreAudio fill path with deterministic
models. Its report is kept outside the repository. It found two mechanisms
that every existing counter misses:

- **Stale reads (A1).** A poll that consumes more than the guest has
  rewritten replays old buffer bytes as good PCM. End to end, with 30 ms
  polls, 45.6% of the frames that reached CoreAudio were stale. At the same
  time `hda CoreAudio stats:` read drops=0 and `hda CoreAudio continuity:`
  read underrun_callbacks=0 and gaps=0.
- **AudioQueue starvation (A2).** With three 10 ms buffers, any callback
  more than 20 ms late starves the device, and no counter sees it.

The investigation also ruled out the 100 ms poll cap and clock drift as causes
of the counted underruns. r52's ~3% counted underruns match a missing start
reserve plus exit-driven bursts, and the 40 ms reserve (#288) removes that in
the model. So the existing records could not show whether an audible stutter is
gone. These four records make poll cadence, the programmed buffer, stale replay
and callback timing visible in every CoreAudio run.

## Records

When the platform drops the HDA controller after the final report, a
controller whose CoreAudio sink installed host records prints these four lines,
in this order (`crates/bridgevm-hvf/src/hda/diagnostics_teardown.rs`). The drop
body runs before the controller's PCM sink drops, so the records land in the
post-footer host tail. They come directly before the sink's lifecycle, stats and
continuity records, which print exactly as before. Each record prints once per
process; resets keep the counters and the host records with the controller.

| Record | Printed by | Fields, in order |
| --- | --- | --- |
| `hda poll:` | `crates/bridgevm-hvf/src/hda/diagnostics.rs` | `polls`, `gap_le_10ms`, `gap_le_20ms`, `gap_le_50ms`, `gap_le_100ms`, `gap_gt_100ms`, `max_gap_ms`, `capped_polls`, `capped_lost_ms`, `ioc_completions`, `max_iocs_per_poll`, `msis_sent` |
| `hda stream:` | the same file | `runs`, `rate_hz`, `channels`, `bits`, `cbl_bytes`, `cbl_ms`, `entries`, `entry_bytes_min`, `entry_bytes_max`, `entry_ms_min`, `entry_ms_max`, `ioc_entries`, `position_buffer`, `lpib_reads`, `running_ms` |
| `hda repeat:` | `crates/bridgevm-hvf/src/hda/repeat_detector.rs` | `passes`, `silent_passes`, `compared_passes`, `repeat_passes`, `max_repeat_run` |
| `hda CoreAudio timing:` | `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/hda_coreaudio_timing.rs` | `callbacks`, `interval_le_10ms`, `interval_le_15ms`, `interval_le_20ms`, `interval_le_30ms`, `interval_le_50ms`, `interval_gt_50ms`, `max_interval_ms`, `late_callbacks`, `trylock_misses`, `ring_samples`, `ring_min_ms`, `ring_max_ms`, `ring_final_ms` |

Values are unsigned decimals. `cbl_ms`, `entry_ms_min` and `entry_ms_max` carry
three decimals, truncated, and `position_buffer` is 0 or 1.

- `hda poll:` counts host-clocked polls while the stream runs. The first poll
  after RUN only takes the time base; each later one is one gap, bucketed
  inclusively at 10, 20, 50 and 100 ms. A gap over 100 ms is a capped poll,
  which plays only 100 ms; `capped_lost_ms` is the host time those polls
  dropped. IOC completions are BDL entries finished with IOC set, and
  `max_iocs_per_poll` is the most that one poll completed. `msis_sent` counts
  MSIs sent while the stream ran.
- `hda stream:` counts RUN starts. For the latest run it records the format,
  CBL and the BDL entries that guest memory held at that run's first poll: up
  to 256 entries, their sizes in bytes and ms, and how many have IOC set. It
  also records whether DPLBASE enabled the DMA position buffer. `lpib_reads`
  counts guest reads that cover SD0 LPIB while the stream runs. `running_ms`
  is the host time the running polls spanned.
- `hda repeat:` hashes every whole BDL-entry pass with 64-bit FNV-1a and
  compares it with that entry's previous lap in the same run. All-zero passes
  count as silent and are never compared, and a pass the DMA joined mid-entry is
  skipped. Content that is truly identical on every lap, such as a tone whose
  period divides the cyclic buffer, also counts as a repeat.
- `hda CoreAudio timing:` covers every output callback before teardown. A
  callback is late when its interval exceeds the 10 ms buffer period by more
  than 20 ms, that is when the interval is over 30 ms, so `late_callbacks`
  equals the last two buckets. `trylock_misses` counts every failed ring
  try_lock; the continuity record counts only those inside a playback span. Ring
  levels are the bytes a callback found before draining, in ms. They are
  sampled from the first callback that plays guest PCM until the guest stops
  its stream.

## Readers

Every post-footer reader now admits each record at most once, in print order and
before any stats or continuity record:

- `scripts/live-gates/hvf_host_tail.py`, which is the T17 tail and the HVF
  binding behind B7, A5 and T17;
- `scripts/hvf-terminal-report.sh`;
- `apps/macos/Sources/BridgeVMProductE2E/HvfHostTail.swift`, with the grammars
  in `apps/macos/Sources/BridgeVMProductE2E/HvfHostDiagnostics.swift`.

The grammars are defined in `scripts/live-gates/hvf_host_diagnostics.py`.

The bounds are unchanged at 16 records and 4 KiB. The widest possible tail puts
u64::MAX in every field, three stopping enqueue failures (one per AudioQueue
buffer), and failed stop and dispose records. It is 11 records and 3,211 bytes,
or 12 and 3,289 with the render server's line. A log printed before the records
existed reads exactly as before, and nothing requires them.

`scripts/live-gates/hda_diagnostics.py [--json] RUN_LOG` reports the records
from the bound host tail only. It refuses any record whose counters do not
reconcile, and exits 1 for a log that has none. It gates nothing.

## Overhead

This is an argument; no overhead was measured.

- **vCPU thread, under the platform lock.**
  - Each running poll does one time subtraction and one bucket increment.
  - Each RUN start reads up to 256 16-byte descriptors, once.
  - Each DMA chunk hashes the bytes already copied for the sink, one xor and
    one multiply per byte. That is 192,000 bytes a second for 48 kHz stereo
    s16.
  - IOC completions and MSIs cost one increment each. A guest register read
    costs one range comparison, plus one relaxed atomic add for an LPIB read
    while running.
- **CoreAudio callback, about 100 a second.**
  - One `Instant::now`, which is a user-space clock read on macOS.
  - Two updates inside a sequence count: a dozen or so relaxed atomic
    operations and two fences.
  - The callback never locks, allocates or waits. Only the reader retries
    when it overlaps an update, and the reader is never the real-time thread.
- **Printing.** Four lines are printed once, after the guest stopped and the
  final report printed.

## Audio behaviour is unchanged

- **No feedback.** No counter value feeds the DMA, interrupt, register or
  callback paths. The sink receives every chunk exactly as before, and the
  callback still copies through `Prefill::drain`.
- **Existing suites, at the base (72fe25bb) and after every commit.**
  `cargo test -p bridgevm-hvf hda` went from 51 to 68 passed, and its 51
  existing tests still pass. The probe example went from 630 to 638 passed, and
  its 630 existing tests still pass.
- **The investigation's deterministic models.** These are the A1 HDA model (3
  tests) and the CoreAudio model (8 tests), both run against the real code.
  Their output is byte-identical between the base and each commit, over 85 and
  219 lines once timing lines are removed.
- **Existing reader suites.** These are the B7 lane test, the continuity
  contract, the stop and media reader contracts, the terminal-stop contracts,
  the T17 tail and counter tests, the A5 classifier and the B7 receipt
  verifier. Each passed the same test count at the base and on this branch.
- **Old-format logs, base readers against branch readers.** On seven logs the
  readers printed byte-identical output: the terminal evidence CLI,
  `scripts/hvf-terminal-report.sh`, `scripts/live-gates/hda_continuity.py` and
  the B7 lane CLI.
- **B7.** The B7 evidence files (`hda_coreaudio.rs`, `hda_coreaudio_stats.rs`,
  `hda_coreaudio_teardown.rs`, `scripts/audio-teardown-result.py`,
  `scripts/verify-audio-teardown-receipt.py`) are unchanged. On ten lanes with
  and without the records, these outputs are identical apart from the run-log
  hashes:
  - the B7 lane JSON;
  - the collected lanes;
  - the receipt, and the verifier's output on it.

## Tests

- **Lib** (`crates/bridgevm-hvf/src/hda/tests`), 17 tests:
  - Polls at gaps of 5, 5 and 250 ms give `gap_le_10ms=2`, `gap_gt_100ms=1`,
    `capped_polls=1` and `capped_lost_ms=150`.
  - The IOC and MSI counts.
  - Exact record strings.
  - The records kept across guest and platform resets.
  - A tagged lap replayed without a rewrite counts `repeat_passes`, while a
    rewritten lap and silence do not.
  - Printing happens only with host records installed, and before the sink
    drops; a replaced controller prints nothing.
- **Probe example**, 8 tests:
  - Injected callback times give the exact histogram and `late_callbacks=3`.
  - Try-lock misses outside a span.
  - Ring levels of 50, 40, 30 and 30 ms.
  - No timing during teardown.
  - Snapshots against a writer running 200,000 updates back to back. This
    test first caught a fallback that returned a torn record.
  - The host record.
  - Byte-identical pins of the lifecycle, stats and enqueue format strings,
    and of the sink's last two statements.
- **Python.**
  - `tests/integration/hda-diagnostics-host-tail-contract.py`, 5 tests. It ties
    each grammar to the Rust format string, the Swift lists and the shell
    patterns, runs every tail reader on 50 cases, and covers B7, A5 and T17.
  - `tests/integration/hda-diagnostics-report-test.py`, 4 tests. All 14
    printed record strings reconcile, 22 unreconciled mutations are refused,
    and guest copies are refused.
- **Swift.** `HvfHostDiagnosticsTests.swift` has 4 tests. With HEAD's readers,
  these new tests failed, and the B7 lane refused a log carrying the records.

## Not verified

- **No live run.** No live single run or gate receipt has these records, so
  real poll gaps, stale-replay rates and callback lateness are unknown.
- **Print order in a real process.** That the records print, in this order,
  when a real probe process drops its platform rests on Rust drop order and lib
  tests with a test sink. It was not observed.
- **Swift.** The Swift cases ran under the repository's XCTest shim, on the
  host-tail, diagnostics and T17 counter suites only. Apple XCTest is not
  installed here.
- **Hosted CI.** Hosted CI has not run, because the branch is not pushed.
- **Project-wide checks.** `scripts/check-project.sh` and the full
  `tests/integration/windows-product-e2e-contract-smoke.sh` runner were not
  run.
