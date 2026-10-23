//! What the ring's 70 ms high-water mark trims: whole frames only, counted
//! byte for byte, as a ring-full drop and as the timing record's
//! `ring_trimmed_ms`.

use super::ring_latency_tests::capped_gap;
use super::*;
use crate::hda_coreaudio::hda_coreaudio_frames::high_water::RING_HIGH_WATER_BYTES;
use crate::hda_coreaudio::hda_coreaudio_frames::PcmFrames;
use crate::hda_coreaudio::hda_coreaudio_prefill::PREFILL_BYTES;
use crate::hda_coreaudio::RING_CAPACITY_BYTES;
use std::collections::VecDeque;

#[test]
fn a_capped_gap_trims_its_burst_and_accounts_for_every_byte() {
    let (shared, written) = capped_gap();
    let t = shared.continuity.timing.snapshot();
    // 100 ms burst onto an empty ring keeps 70; until the next callback
    // drains it, at most 10 ms more arrive at the mark.
    assert!((30..=41).contains(&t.ring_trimmed_ms), "{t:?}");
    let trimmed = shared.continuity.timing.ring_trimmed.load(Relaxed) as usize;
    let rendered = shared.frames_rendered.load(Relaxed) as usize * 4;
    // Every byte the controller wrote reached the ring or was trimmed,
    // except a partial frame still private to the producer.
    let private = written.checked_sub(rendered + trimmed);
    assert!(
        matches!(private, Some(0..=3)),
        "{written} {rendered} {trimmed}"
    );
}

#[test]
fn a_trim_drops_whole_frames_and_keeps_the_source_phase() {
    let (capacity, mark) = (RING_CAPACITY_BYTES, RING_HIGH_WATER_BYTES);
    assert_eq!(mark, 70 * 192, "70 ms of 48 kHz stereo s16le");
    let mut frames = PcmFrames::default();
    let mut ring = VecDeque::from(vec![0; mark - 8]);
    let source: Vec<u8> = (1..=24).collect();
    // Two frames of room: 1-8 go in, 9-12 are trimmed as a ring-full drop,
    // 13-14 stay private.
    let (published, trimmed) = frames.write_bounded(&source[..14], &mut ring, capacity, mark);
    assert_eq!(
        (published.guest_frames, published.dropped_bytes, trimmed),
        (2, 4, 4)
    );
    assert!(ring.iter().skip(mark - 8).copied().eq(1..=8));
    // At the mark, the frame the private bytes complete is trimmed whole.
    let (published, trimmed) = frames.write_bounded(&source[14..17], &mut ring, capacity, mark);
    assert_eq!(
        (published.guest_frames, published.dropped_bytes, trimmed),
        (0, 4, 4)
    );
    assert_eq!(ring.len(), mark);
    // Room again: the next frame starts where the source's frames do.
    ring.drain(..8);
    let (published, trimmed) = frames.write_bounded(&source[17..], &mut ring, capacity, mark);
    assert_eq!(
        (published.guest_frames, published.dropped_bytes, trimmed),
        (2, 0, 0)
    );
    assert!(ring.iter().skip(mark - 8).copied().eq(17..=24));
    // Below the mark it is `write`, byte for byte.
    let (mut bounded, mut plain) = (PcmFrames::default(), PcmFrames::default());
    let (mut ring, mut expected) = (VecDeque::new(), VecDeque::new());
    for chunk in source.chunks(5) {
        let (published, trimmed) = bounded.write_bounded(chunk, &mut ring, capacity, mark);
        assert_eq!(
            (published, trimmed),
            (plain.write(chunk, &mut expected, capacity), 0)
        );
    }
    assert_eq!(ring, expected);
}

#[test]
fn the_timing_record_reports_trimmed_milliseconds_rounded_up() {
    let shared = Shared::new(RING_CAPACITY_BYTES, PREFILL_BYTES);
    let trimmed = || shared.continuity.timing.snapshot().ring_trimmed_ms;
    assert_eq!(trimmed(), 0);
    shared.continuity.note_trimmed(4);
    assert_eq!(trimmed(), 1, "one trimmed frame reads as 1 ms, not 0");
    shared.continuity.note_trimmed(30 * 192 - 4);
    assert_eq!(trimmed(), 30);
    let record = shared.continuity.timing.snapshot().record();
    assert!(
        record.ends_with(" ring_final_ms=0 ring_trimmed_ms=30"),
        "{record}"
    );
}
