use super::super::hda_coreaudio_continuity::fill_and_record;
use super::super::hda_coreaudio_producer::PcmProducer;
use super::*;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn rejected_format_ends_carry_without_counting_padding_as_guest_frame() {
    let shared = Shared::new(32, 0);
    let mut producer = PcmProducer::default();
    producer.write_format(&[1, 2], (48_000, 2, 16), &shared, 32);
    producer.write_format(&[90, 91, 92], (44_100, 2, 16), &shared, 32);
    producer.write_format(&[3, 4, 5, 6], (48_000, 2, 16), &shared, 32);
    let mut out = [0; 8];
    fill_and_record(&mut out, &shared);
    assert_eq!(out, [1, 2, 0, 0, 3, 4, 5, 6]);
    assert_eq!(shared.frames_rendered.load(Relaxed), 1);
    assert_eq!(shared.dropped_writes.load(Relaxed), 1);
    assert_eq!(shared.dropped_bytes.load(Relaxed), 3);
    assert_eq!(shared.format_drops.load(Relaxed), 1);
    assert_eq!(shared.ring_full_drops.load(Relaxed), 0);
}

#[test]
fn loss_counters_count_one_publication_batch_and_only_source_bytes() {
    let shared = Shared::new(4, 0);
    let mut producer = PcmProducer::default();
    producer.write(&[90; 4], &shared, 4);
    producer.write(&[1, 2], &shared, 4);
    producer.write(&[3, 4, 5, 6, 7, 8, 9], &shared, 4);
    producer.finish(&shared, 4);
    producer.finish(&shared, 4);
    assert_eq!(shared.frames_rendered.load(Relaxed), 1);
    assert_eq!(shared.dropped_writes.load(Relaxed), 2);
    assert_eq!(shared.dropped_bytes.load(Relaxed), 9);
    assert_eq!(shared.format_drops.load(Relaxed), 0);
    assert_eq!(shared.ring_full_drops.load(Relaxed), 2);
    assert!(shared.continuity.stream_idle());
}
