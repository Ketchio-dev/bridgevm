# HDA service tick and CoreAudio ring high-water mark — 2026-10-23

Classification: audio fixes 1 and 3 of the 2026-10-21 audio investigation.
T17 r92's host-record analysis (2026-10-23, sections 1-3) supports fix 1 and
then fix 3, and refutes fix 4 for that run. Implemented test-first on the base
`63ad6ce5` in `984ced08` (fix 1) and `cfbdb930` (fix 3). Design choices were
delegated by the operator; each is recorded here. **No live run has exercised
either change.** No criterion, registry entry, capability wording or product
state changes. Ranks are those of `AGENTS.md`: rank 3 is an automated test,
rank 4 static reasoning. Red and green logs are kept outside the repository
with the other evidence of this work.

## 1. What changed

### The HDA service tick (`984ced08`)

HDA playback DMA advances by the host time between polls, and before this only
a vCPU exit's drain polled it. r92's stream ran 5807 ms with 54 poll gaps of
20-50 ms, one of 50-100 ms and one of 216 ms against Windows' 20 ms cyclic
buffer: the DMA re-read audio the guest had not rewritten, the 100 ms cap
dropped 116 ms, and IOCs that one poll crossed shared one MSI.

- **Stream-activity flag** (`crates/bridgevm-hvf/src/hda/stream_activity.rs`):
  a lock-free flag that rises with SD0CTL.RUN (`stream_control.rs`) and
  clears on RUN clear, stream reset, each of the three DMA halts
  (`playback_dma.rs`), controller reset and platform reset. The handle
  survives `replace_keeping_diagnostics`, so the tick keeps watching across
  resets. A rise unparks the one registered waiter.
- **Platform poll that takes only HDA's MSI**
  (`crates/bridgevm-hvf/src/platform_virt/audio_service_tick.rs`): sets the
  host time, polls HDA over the given memory and drains HDA's MSI into a local
  message. It never touches the shared `pending_msix` the vCPU drains use.
- **The tick thread**
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/hda_service_tick.rs`):
  parked until the flag rises; while it is set, every 5 ms on a deadline it
  `try_lock`s the platform (a busy platform skips the tick), polls with the
  host time over the host RAM mapping, takes only HDA's MSI, unlocks, and
  then sends the MSI through `deliver_pending_msix`, the vCPU drains' path to
  `hv_gic_send_msi`. It runs at user-interactive QoS and never calls
  `hv_vcpus_exit`. Under `BRIDGEVM_USERSPACE_GIC` an MSI send is a vCPU kick,
  so there it stays off and says so. It prints
  `hda service tick: started interval_ms=5` and, as it ends,
  `hda service tick: stopped streams= ticks= busy= quiesced= msis= max_late_us=`.
- **Placement** (`probe_runtime.rs`): started next to
  `RamfbDisplayThread::start`, outside the reboot loop; the reboot arm takes
  `quiesce()` first and holds it across the GIC and platform resets; the
  terminal path drops it right after the display thread and before the final
  report, so its stop line prints before the footer.
- **Deviation entry** `hda-dma-pacing` in
  `docs/machine-contract/qemu-virt-deviations-audio.json`, listed first in the
  `deviation_modules` of `docs/machine-contract/qemu-virt-deviations.json`.
  Guest-visible: DMA position and IOC interrupts advance on a 5 ms host tick
  while a stream runs, and on vCPU exits otherwise or under the userspace GIC.
  QEMU (master, read 2026-10-23 at integration): a 1 ms virtual-clock timer.

### The ring's 70 ms high-water mark (`cfbdb930`)

r92's ring ended at 104 ms, 2.6 times the 40 ms reserve, with no contention
and no late callback: the one capped poll wrote 100 ms onto a drained ring and
nothing brought the level back.

- **The mark**
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/hda_coreaudio_high_water.rs`):
  the producer appends until the ring would pass 70 ms (13,440 B: the 40 ms
  reserve, the 10.7 ms device-cycle sawtooth and margin) and trims the whole
  frames beyond it. The partial frame after them stays private, so accepted
  bytes keep their frame phase. Below the mark the write is unchanged byte
  for byte, and a batch the 500 ms ring cannot take is still rejected whole.
  The trim runs in the producer, never in the real-time callback.
- **Decision: a trim counts as a ring-full drop.** A trim is guest PCM that
  never reached the ring, which A5 and T17 read as `drops`. Before the mark
  the same excess grew the ring until it overflowed into `ring_full_drops`;
  the mark makes that overflow unreachable, so leaving trims out of `drops`
  would have relaxed A5. Each trimming write is published as one ring-full
  drop of its trimmed bytes: `drops==0` is exactly as strict as before, B7's
  `drops == format_drops + ring_full_drops` still reconciles, and the stats
  record prints byte-identically. The A5 and B7 evidence files
  (`hda_coreaudio.rs`, `hda_coreaudio_stats.rs`, `hda_coreaudio_teardown.rs`
  and the two B7 scripts) are unchanged.
- **`ring_trimmed_ms`** is the new last field of `hda CoreAudio timing:`: the
  trimmed bytes in ms, rounded up so any trim reads at least 1. The producer
  adds to its own relaxed counter outside the callback's sequence count.
  Every grammar changed together: the Rust printer, `HvfHostDiagnostics.swift`
  (read by `HvfHostRecordFields.swift`), `hvf_host_diagnostics.py` (read by
  `product_e2e_host_records.py`), `scripts/hvf-terminal-report.sh`, and the
  fixtures of the host-tail contracts and lane tests. The widest HDA tail
  grows 37 B to 3,248 B, or 3,717 B with the display record, inside the
  unchanged 4,096 B and 16-line bounds.

## 2. Why the tick cannot deadlock, hold the lock across delivery, or outlive the run

- **Locks.** The tick only `try_lock`s the platform and holds it only to poll
  and take HDA's MSI; it drops the guard before `deliver_pending_msix`. vCPUs
  never take the tick's gate; the reboot arm takes the gate and then the
  platform. The tick holds its gate across a tick but never waits on any lock
  while holding it, so no wait-for cycle exists and `quiesce()` waits at most
  one tick [rank 4; tests below, rank 3].
- **vCPU exits.** The tick holds no vCPU handle and never calls
  `hv_vcpus_exit`, so it adds no EXIT_CANCELED and no roll of the A1 swallow
  race. Each MSI leaves the controller exactly once, under the platform
  lock: into the vCPU drains' `pending_msix`, or into the tick's own message. `hv_gic.h` states an owning-thread rule for the
  redistributor and CPU-interface accessors and none for `hv_gic_send_msi`
  [rank 4].
- **Lifetime.** One per probe process, joined by its drop before the final
  report and, on unwinding, before guest RAM is unmapped; the reboot arm's
  `quiesce()` keeps an MSI taken in one boot generation out of the next.
  A failed send or a poisoned platform ends the thread with one line, and HDA
  falls back to vCPU exits.
- **The ring.** `write_pcm` runs under the platform lock on a vCPU or the
  tick, so there is still one producer at a time; the callback still only
  `try_lock`s the ring.

## 3. Tests, red then green

All new tests are in new files. Red runs stub the behaviour behind unchanged
seams (fix 1) or run the new test on the commit before the change (fix 3).

| Test | Red | Green |
| --- | --- | --- |
| 4 stream-activity and 3 service-poll lib tests | 6 of 7 failed; the no-HDA case passes by design (`step1-red.log`) | 7 passed (`step1-green.log`) |
| `hda_tick_advances_stream_without_vcpu_exits` (REPORT a) | failed | passed |
| `hda_tick_skips_when_platform_is_locked`, `hda_tick_is_parked_without_running_stream`, `hda_tick_never_requests_vcpu_exit`, `hda_tick_lifecycle_spans_reboot_generations_and_joins_before_unmap` (REPORT b-e) | failed | passed |
| `hda_tick_bounds_gaps_under_live_burst_schedule` | gap_le_50ms=54, gap_le_100ms=1, gap_gt_100ms=1, capped_polls=1 losing 116 ms, repeat_passes=104, 113 MSIs for 236 IOCs | 6252 polls all within 10 ms, max gap 5 ms; capped and repeats 0; 248 MSIs for 248 IOCs |
| `the_tick_never_holds_the_platform_lock_while_sending`, `vcpu_exits_and_the_tick_never_deadlock_or_duplicate_msis`, `a_quiesced_tick_neither_polls_nor_sends_and_loses_no_time`, `the_probe_runs_one_tick_and_joins_it_before_the_report` | failed | passed (11 example tests, `step1-green.log`) |
| `unwinding_joins_the_tick` | failed: timed out waiting for ticks (`step1-red-unwinding.log`) | passed |
| `capped_gap_does_not_hold_ring_latency` | at `984ced08` plus only this test file and its mod lines: ring_max_ms=104, ring_final_ms=104, r92's live 104 ms (`step2-red.log`, `step2-red-setup.diff`) | at most 70 (`step2-green.log`) |
| `a_capped_gap_trims_its_burst_and_accounts_for_every_byte`, `a_trim_drops_whole_frames_and_keeps_the_source_phase`, `the_timing_record_reports_trimmed_milliseconds_rounded_up` | need the new API | passed |

Counts: probe example 708, 719 after fix 1, 723 after fix 3; bridgevm-hvf lib
1483 to 1490 passed, 2 ignored; venus lib 1517 summed result lines after each
step. On each step `cargo fmt --check`, both clippy runs with `-D warnings`,
the per-file rustfmt comparison with the base, the Python checks and the
machine-contract schema checks passed; after fix 3, which alone changed the
records and Swift, so did the host-tail, lane and private-keys contracts and
the xctest shim suites. The shim's first fix-3 run failed one assertion that
counts each record's fields, [12, 15, 5, 14]; the timing record now has 15
fields, and the count was updated (`step2-xctest-shim-first.log`).

## 4. What the next T17 first run must show

- `hda service tick: started interval_ms=5` before the report, and its stop
  line with `streams` of at least 1, `msis` above 0 and `quiesced=0` unless
  the guest rebooted.
- `hda poll:` with `gap_le_50ms=0`, `gap_le_100ms=0`, `gap_gt_100ms=0` and
  `capped_polls=0`; `msis_sent` near `ioc_completions` and
  `max_iocs_per_poll=1`.
- `hda repeat:` with `repeat_passes=0`.
- `hda CoreAudio timing:` with `ring_min_ms` above 0, `ring_max_ms` at most
  70 and `ring_trimmed_ms=0`; the stats record with `drops=0`.
- For fix 3's long-session claim, `ring_full_drops=0` over at least 30
  minutes; that is where device-clock drift would show.

## 5. Not verified

- No live run: neither the tick nor the mark has met a real guest, Apple's
  GIC or a real AudioQueue device cycle.
- `hv_gic_send_msi` from a non-vCPU thread rests on the header's silence,
  not on a live run.
- QEMU's side: recalled here, then verified against QEMU master at integration.
- The controller-reset branch of `machine_assembly.rs` (`reset()`) was not
  fully read; the platform-reset test covers the flag's handle.
- The wake latency gained from user-interactive QoS is not measured.
- Whether a trim is audible, and whether 70 ms leaves room for live callback
  jitter, is modelled only. A device clock slower than the host's still
  trims, and by the decision above that fails A5's and T17's `drops==0`; a
  sample-rate converter remains the fix for drift.
- B7's ten-run campaign was not re-run; its evidence files are unchanged.
- `hda_coreaudio.rs`'s header still says `write_pcm` runs on the vCPU
  thread. It was left as it is because the file is an A5 and B7 evidence
  path; the producer and tick documentation name both callers.
