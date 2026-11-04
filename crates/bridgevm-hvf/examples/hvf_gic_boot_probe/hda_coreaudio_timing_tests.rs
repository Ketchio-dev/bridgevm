//! Callback timing semantics and the `hda CoreAudio timing:` record.

use super::*;
use crate::hda_coreaudio::hda_coreaudio_continuity::fill_and_record;
use crate::hda_coreaudio::hda_coreaudio_prefill::PREFILL_BYTES;
use crate::hda_coreaudio::hda_coreaudio_producer::PcmProducer;
use crate::hda_coreaudio::{AUDIO_QUEUE_BUFFER_BYTES, RING_CAPACITY_BYTES};
use std::time::Duration;
#[path = "hda_coreaudio_teardown_record_tests.rs"]
mod teardown_records;

const FULL: usize = AUDIO_QUEUE_BUFFER_BYTES as usize;
const IDLE: &str = "hda CoreAudio timing: callbacks=0 interval_le_10ms=0 interval_le_15ms=0 \
                    interval_le_20ms=0 interval_le_30ms=0 interval_le_50ms=0 interval_gt_50ms=0 \
                    max_interval_ms=0 late_callbacks=0 trylock_misses=0 ring_samples=0 \
                    ring_min_ms=0 ring_max_ms=0 ring_final_ms=0";

fn feed(shared: &Shared, bytes: usize) {
    PcmProducer::default().write(&vec![0x5a; bytes], shared, RING_CAPACITY_BYTES);
}

#[test]
fn injected_callback_times_give_the_exact_interval_histogram_and_late_count() {
    let base = Instant::now();
    let timing = CallbackTiming::starting_at(base);
    timing.note_callback(base, false);
    let mut at = Duration::ZERO;
    let intervals_us = [
        10_000, 10_500, 15_000, 20_000, 20_500, 30_000, 30_500, 50_000, 51_000,
    ];
    for interval in intervals_us.map(Duration::from_micros) {
        at += interval;
        timing.note_callback(base + at, false);
    }
    // More than 20 ms late is an interval over 30 ms: 30.5, 50 and 51 ms.
    assert_eq!(
        timing.snapshot().record(),
        "hda CoreAudio timing: callbacks=10 interval_le_10ms=1 interval_le_15ms=2 \
         interval_le_20ms=1 interval_le_30ms=2 interval_le_50ms=2 interval_gt_50ms=1 \
         max_interval_ms=51 late_callbacks=3 trylock_misses=0 ring_samples=0 ring_min_ms=0 \
         ring_max_ms=0 ring_final_ms=0"
    );
    assert_eq!(CallbackTiming::starting_at(base).snapshot().record(), IDLE);
}

#[test]
fn a_paused_queue_is_not_a_late_callback() {
    let base = Instant::now();
    let timing = CallbackTiming::starting_at(base);
    timing.note_callback(base, false);
    timing.note_callback(base + Duration::from_millis(10), false);
    // Paused for 30 s, then started again by the output supervisor.
    timing.note_restart();
    timing.note_callback(base + Duration::from_secs(30), false);
    timing.note_callback(base + Duration::from_millis(30_010), false);
    let snapshot = timing.snapshot();
    assert_eq!((snapshot.callbacks, snapshot.intervals[0]), (4, 2));
    assert_eq!((snapshot.late_callbacks, snapshot.max_interval_ms), (0, 10));
}

#[test]
fn every_busy_consumer_counts_even_where_continuity_does_not() {
    let shared = Shared::new(4 * FULL, 0);
    let mut buffer = vec![0u8; FULL];
    {
        let _consumer = shared.ring.try_consumer().unwrap();
        fill_and_record(&mut buffer, &shared);
        fill_and_record(&mut buffer, &shared);
    }
    fill_and_record(&mut buffer, &shared);
    let timing = shared.continuity.timing.snapshot();
    assert_eq!((timing.callbacks, timing.trylock_misses), (3, 2));
    // Before any PCM there is no span, so the continuity record counts none.
    assert_eq!(shared.continuity.snapshot().contention_callbacks, 0);
}

#[test]
fn ring_levels_are_sampled_only_while_the_fed_stream_plays() {
    let shared = Shared::new(RING_CAPACITY_BYTES, PREFILL_BYTES);
    let mut buffer = vec![0u8; FULL];
    feed(&shared, 2 * FULL);
    fill_and_record(&mut buffer, &shared); // 20 ms is below the 40 ms start reserve
    feed(&shared, 3 * FULL);
    for _ in 0..3 {
        fill_and_record(&mut buffer, &shared); // finds 50, 40 and 30 ms
    }
    feed(&shared, FULL);
    fill_and_record(&mut buffer, &shared); // finds 30 ms
    shared.continuity.note_stream_stopped();
    fill_and_record(&mut buffer, &shared); // the guest stopped: no sample
    let t = shared.continuity.timing.snapshot();
    let ring = (
        t.ring_samples,
        t.ring_min_ms,
        t.ring_max_ms,
        t.ring_final_ms,
    );
    assert_eq!(ring, (4, 30, 50, 30));
    assert_eq!(t.callbacks, 6);
}

#[test]
fn teardown_callbacks_are_neither_timed_nor_sampled() {
    let shared = Shared::new(4 * FULL, 0);
    let mut buffer = vec![0u8; FULL];
    feed(&shared, 3 * FULL);
    fill_and_record(&mut buffer, &shared);
    shared.continuity.begin_stopping();
    fill_and_record(&mut buffer, &shared);
    let t = shared.continuity.timing.snapshot();
    assert_eq!((t.callbacks, t.ring_samples, t.ring_final_ms), (1, 1, 30));
}

#[test]
fn a_snapshot_taken_while_callbacks_run_is_consistent() {
    let base = Instant::now();
    let timing = Arc::new(CallbackTiming::starting_at(base));
    let writer = {
        let timing = Arc::clone(&timing);
        std::thread::spawn(move || {
            let mut at = Duration::ZERO;
            for n in 0..200_000u64 {
                at += Duration::from_millis(5 + n % 50);
                timing.note_callback(base + at, n % 9 == 0);
                timing.note_drain((n % 4_000) as usize * 4, FULL, false);
            }
        })
    };
    let mut snapshots = 0u64;
    while !writer.is_finished() || snapshots == 0 {
        let s = timing.snapshot();
        let intervals: u64 = s.intervals.iter().sum();
        assert_eq!(intervals, s.callbacks.saturating_sub(1), "{s:?}");
        assert_eq!(s.late_callbacks, s.intervals[4] + s.intervals[5], "{s:?}");
        assert!(s.trylock_misses <= s.callbacks, "{s:?}");
        let ordered = s.ring_min_ms <= s.ring_final_ms && s.ring_final_ms <= s.ring_max_ms;
        assert!(ordered || s.ring_samples == 0, "{s:?}");
        snapshots += 1;
    }
    writer.join().unwrap();
    assert_eq!(timing.snapshot().callbacks, 200_000);
}

#[test]
fn host_records_report_the_live_sinks_timing_and_nothing_once_it_is_gone() {
    let shared = Arc::new(Shared::new(4 * FULL, 0));
    let weak = Arc::downgrade(&shared);
    assert_eq!(host_records(&weak), [IDLE]);
    drop(shared);
    assert!(host_records(&weak).is_empty());
}
