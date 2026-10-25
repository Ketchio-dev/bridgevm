# The ring's 70 ms high-water mark is withdrawn — 2026-10-24

Classification: T17 r94, a live single run at seal `79da0472` [rank 2],
failed its audio stage on the CoreAudio ring's 70 ms high-water mark (audio
fix 3, `cfbdb930`, recorded in
`docs/history/windows-hvf/hda-service-tick-20261023.md`). The operator
decided to withdraw the mark and keep the HDA service tick (fix 1). This is
done test-first in `1b429cc2`, `9253aaf8` and `ce054a6d`. **No live run has
exercised the withdrawal.** No criterion, registry entry, capability wording
or product state changes.

## 1. What r94 showed

In the journey's first-ready session the host printed:

- `hda service tick: stopped streams=2 ticks=1199 busy=835 quiesced=0 msis=100`:
  another thread held the platform lock at 70% of the ticks;
- `hda poll:` with `gap_le_20ms=78 gap_le_50ms=33 gap_le_100ms=2
  gap_gt_100ms=5 max_gap_ms=394 capped_polls=5 capped_lost_ms=631`, and
  `msis_sent=393` for 536 IOCs;
- `hda CoreAudio timing:` with `ring_max_ms=70 ring_trimmed_ms=178`;
- `hda CoreAudio stats: frames_rendered=249264 drops=275 ...
  ring_full_drops=275`, so T17's audio counters failed.

An earlier session of the same run had one capped gap, 48 ms trimmed and
289 drops. In the first-ready session the display exporter also woke 119 ms
late once (`max_late_us=118657`) [rank 2].

## 2. Why the mark goes

A5, which is PROVEN, reads "Guest PCM reaches the host CoreAudio ring with
frames_rendered>0 AND drops==0". Trimmed PCM never reaches the ring, so
counting each trim as a ring-full drop was the right accounting. But a
capped gap refills the ring past 70 ms in one poll, so the mark fails A5
whenever a capped gap happens. The tick cannot prevent capped gaps while
another thread holds the platform lock, which r94 shows it often does.
Without the mark, a capped gap's burst stays in the ring as latency again
(r92: 104 ms). That latency creep is open again. Closing it without
dropping PCM would need a sample-rate converter or a re-prime that keeps
every byte.

## 3. What changed

- `1b429cc2`: new test
  `a_capped_gap_delivers_every_polled_byte_to_the_ring_without_drops`
  (`hda_coreaudio_capped_gap_a5_tests.rs`). It runs the real controller,
  producer and callback fill path, with 0.2 ms polls for 4 s and one 216 ms
  stall that the controller records as one capped poll losing 116 ms. Every
  byte the controller wrote must reach the ring, except a partial frame the
  producer still holds, and drops, dropped_bytes, format_drops and
  ring_full_drops must all be 0. The four tests of the withdrawn behaviour
  are removed. On that commit, which still trims, the test fails with
  `written=745688 rendered=739160 drops=[23, 6528, 0, 23]` [rank 3].
- `9253aaf8`: the producer writes with `PcmFrames::write` again, and
  `hda_coreaudio_high_water.rs` is deleted. `ring_trimmed_ms` is removed
  from every grammar of the `hda CoreAudio timing:` record at once: the Rust
  printer, `HvfHostDiagnostics.swift`, `hvf_host_diagnostics.py`,
  `hvf-terminal-report.sh` and the contract fixtures. The record has its 14
  fields again. The widest HDA tail is back to 3,211 B (3,680 B with the
  display record). The test now passes [rank 3].
- `ce054a6d`: `hda service tick: stopped` ends with `max_busy_run_ms`. This
  is the longest run of consecutive ticks that found the platform busy,
  times the 5 ms interval. A tick that gets the lock, a quiesced tick or a
  new stream ends a run. No reader or contract parses this line. Its three
  tests failed with the run tracking stubbed out, apart from the wiring
  check, and pass now [rank 3].

## 4. What the next T17 run must show

- `hda CoreAudio stats:` with `drops=0` and `ring_full_drops=0`, so T17's
  audio counters and A5 pass again.
- `hda service tick: stopped ... max_busy_run_ms=`, read against
  `hda poll:`'s `max_gap_ms`. A busy run about as long as the longest gap
  means one lock holder caused it. Short runs mean the gaps came from
  somewhere else, as the display exporter's 119 ms late wake might.
- `ring_max_ms` above 70 after a capped gap is expected now. It is
  latency, not loss.

## 5. Not verified

- No live run has exercised the withdrawal or `max_busy_run_ms`.
- Which thread holds the platform lock for r94's busy ticks is unknown.
  `max_busy_run_ms` only measures how long each hold lasts ([later](platform-lock-holds-20261025.md)).
- The display exporter's 119 ms late wake is unexplained.
- The latency creep after a capped gap is unfixed again.
