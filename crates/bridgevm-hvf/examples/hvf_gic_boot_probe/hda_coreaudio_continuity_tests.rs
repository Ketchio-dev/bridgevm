//! Continuity counter semantics and the printed record.

use super::super::hda_coreaudio_stats::Shared;
use super::*;

const FULL: usize = AUDIO_QUEUE_BUFFER_BYTES as usize;
const HALF: usize = FULL / 2;

fn drained(pcm_bytes: usize) -> CallbackFill {
    CallbackFill::Drained {
        pcm_bytes,
        capacity_bytes: FULL,
        stream_idle: false,
    }
}

fn idle(pcm_bytes: usize) -> CallbackFill {
    CallbackFill::Drained {
        pcm_bytes,
        capacity_bytes: FULL,
        stream_idle: true,
    }
}

const CONTENDED: CallbackFill = CallbackFill::Contended {
    capacity_bytes: FULL,
};

fn run(fills: &[CallbackFill]) -> ContinuitySnapshot {
    let counters = ContinuityCounters::default();
    for fill in fills {
        counters.record(*fill);
    }
    counters.snapshot()
}

fn snapshot(
    active: u64,
    underruns: u64,
    frames: u64,
    contention: u64,
    gaps: u64,
    max: u64,
) -> ContinuitySnapshot {
    ContinuitySnapshot {
        active_callbacks: active,
        underrun_callbacks: underruns,
        underrun_frames: frames,
        contention_callbacks: contention,
        gaps,
        max_gap_frames: max,
        stream_stops: 0,
        callback_frames: 480,
    }
}

#[test]
fn warm_up_silence_and_contention_before_the_first_pcm_are_not_counted() {
    assert_eq!(
        run(&[idle(0), idle(0), drained(0), CONTENDED]),
        snapshot(0, 0, 0, 0, 0, 0)
    );
}

#[test]
fn full_buffers_count_as_active_without_underruns() {
    assert_eq!(
        run(&[idle(0), drained(FULL), drained(FULL), drained(FULL)]),
        snapshot(3, 0, 0, 0, 0, 0)
    );
}

#[test]
fn partial_underrun_followed_by_pcm_is_one_gap() {
    assert_eq!(
        run(&[drained(FULL), drained(HALF), drained(FULL)]),
        snapshot(3, 1, 240, 0, 1, 240)
    );
}

#[test]
fn empty_callbacks_between_pcm_extend_one_gap() {
    let fills = [
        drained(FULL),
        drained(HALF),
        drained(0),
        drained(0),
        drained(FULL),
    ];
    assert_eq!(run(&fills), snapshot(5, 3, 240 + 960, 0, 1, 1200));
}

#[test]
fn separate_gaps_are_counted_and_the_longest_is_kept() {
    // The half buffer's PCM ends the two-buffer gap; its own tail opens a third.
    let fills = [
        drained(FULL),
        drained(0),
        drained(FULL),
        drained(0),
        drained(0),
        drained(HALF),
        drained(FULL),
    ];
    assert_eq!(run(&fills), snapshot(7, 4, 480 + 960 + 240, 0, 3, 960));
}

#[test]
fn contention_inside_a_span_is_an_underrun_of_a_whole_buffer() {
    let fills = [
        drained(FULL),
        CONTENDED,
        drained(FULL),
        CONTENDED,
        drained(0),
        drained(FULL),
    ];
    assert_eq!(run(&fills), snapshot(6, 3, 1440, 2, 2, 960));
}

#[test]
fn trailing_silence_without_later_pcm_is_never_counted() {
    assert_eq!(
        run(&[drained(FULL), drained(HALF), drained(0), CONTENDED]),
        snapshot(2, 0, 0, 0, 0, 0)
    );
}

#[test]
fn a_guest_stream_stop_closes_the_span_and_its_idle_silence_is_not_a_gap() {
    let counters = ContinuityCounters::default();
    for fill in [drained(FULL), drained(0)] {
        counters.record(fill);
    }
    counters.note_stream_stopped();
    assert!(counters.stream_idle());
    // The ring drains the last pre-stop PCM, then idles until the guest restarts.
    for fill in [idle(HALF), idle(0), CONTENDED, idle(0)] {
        counters.record(fill);
    }
    counters.note_pcm_written();
    assert!(!counters.stream_idle());
    for fill in [drained(0), drained(FULL), drained(HALF), drained(FULL)] {
        counters.record(fill);
    }
    // The empty buffer before the stop was a gap: pre-stop PCM followed it.
    let expected = ContinuitySnapshot {
        stream_stops: 1,
        ..snapshot(6, 2, 480 + 240, 0, 2, 480)
    };
    assert_eq!(counters.snapshot(), expected);
}

#[test]
fn a_stop_with_pcm_still_buffered_keeps_counting_until_the_ring_runs_short() {
    // Idle but full buffers are still the pre-stop stream playing out.
    let fills = [
        drained(FULL),
        idle(FULL),
        idle(FULL),
        idle(HALF),
        drained(FULL),
    ];
    assert_eq!(run(&fills), snapshot(5, 0, 0, 0, 0, 0));
}

#[test]
fn teardown_stops_counting_and_discards_pending_silence() {
    let counters = ContinuityCounters::default();
    for fill in [drained(FULL), drained(0)] {
        counters.record(fill);
    }
    counters.begin_stopping();
    counters.record(drained(FULL));
    assert_eq!(counters.snapshot(), snapshot(1, 0, 0, 0, 0, 0));
}

#[test]
fn a_partial_frame_of_silence_counts_as_one_silent_frame() {
    assert_eq!(
        run(&[drained(FULL), drained(FULL - 2), drained(FULL)]),
        snapshot(3, 1, 1, 0, 1, 1)
    );
}

#[test]
fn record_prints_every_field_in_the_documented_order() {
    let value = ContinuitySnapshot {
        stream_stops: 7,
        ..snapshot(1, 2, 3, 4, 5, 6)
    };
    assert_eq!(
        value.record(),
        "hda CoreAudio continuity: active_callbacks=1 underrun_callbacks=2 underrun_frames=3 \
         contention_callbacks=4 gaps=5 max_gap_frames=6 stream_stops=7 callback_frames=480"
    );
    assert_eq!(
        ContinuityCounters::default().snapshot().record(),
        "hda CoreAudio continuity: active_callbacks=0 underrun_callbacks=0 underrun_frames=0 \
         contention_callbacks=0 gaps=0 max_gap_frames=0 stream_stops=0 callback_frames=480"
    );
}

#[test]
fn fill_reads_the_ring_and_records_what_the_buffer_received() {
    let shared = Shared::new(4 * FULL, 0);
    let mut buffer = vec![0u8; FULL];
    fill_and_record(&mut buffer, &shared);
    assert_eq!(
        buffer,
        vec![0u8; FULL],
        "an empty ring leaves the buffer silent"
    );
    for bytes in [FULL, FULL + HALF] {
        let mut ring = shared.ring.lock().unwrap();
        ring.extend(std::iter::repeat_n(0x5a, bytes));
        shared.continuity.note_pcm_written();
    }
    for expected in [FULL, FULL, HALF] {
        let mut buffer = vec![0u8; FULL];
        fill_and_record(&mut buffer, &shared);
        assert_eq!(
            buffer.iter().filter(|&&byte| byte == 0x5a).count(),
            expected
        );
    }
    shared
        .ring
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(0x5a, FULL));
    fill_and_record(&mut buffer, &shared);
    assert_eq!(shared.continuity.snapshot(), snapshot(4, 1, 240, 0, 1, 240));
}

#[test]
fn fill_never_waits_for_the_producer_and_counts_the_contention() {
    let shared = Shared::new(4 * FULL, 0);
    shared
        .ring
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(0x5a, 3 * FULL));
    shared.continuity.note_pcm_written();
    let mut buffer = vec![0u8; FULL];
    fill_and_record(&mut buffer, &shared);
    {
        let _producer = shared.ring.lock().unwrap();
        let mut silent = vec![0u8; FULL];
        fill_and_record(&mut silent, &shared);
        assert_eq!(
            silent,
            vec![0u8; FULL],
            "a busy ring leaves the buffer silent"
        );
    }
    fill_and_record(&mut buffer, &shared);
    assert_eq!(shared.continuity.snapshot(), snapshot(3, 1, 480, 1, 1, 480));
}

#[test]
fn fill_reads_a_guest_stop_under_the_ring_lock() {
    let shared = Shared::new(4 * FULL, 0);
    shared
        .ring
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(0x5a, FULL + HALF));
    shared.continuity.note_pcm_written();
    shared.continuity.note_stream_stopped();
    let mut buffer = vec![0u8; FULL];
    for _ in 0..3 {
        fill_and_record(&mut buffer, &shared);
    }
    shared
        .ring
        .lock()
        .unwrap()
        .extend(std::iter::repeat_n(0x5a, FULL));
    shared.continuity.note_pcm_written();
    fill_and_record(&mut buffer, &shared);
    let expected = ContinuitySnapshot {
        stream_stops: 1,
        ..snapshot(3, 0, 0, 0, 0, 0)
    };
    assert_eq!(shared.continuity.snapshot(), expected);
}
