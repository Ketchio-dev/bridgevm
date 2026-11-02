//! Whole-frame capacity, wrap and publication boundaries of the actual PCM ring.

use super::super::hda_coreaudio_continuity::fill_and_record;
use super::super::hda_coreaudio_producer::PcmProducer;
use super::super::hda_coreaudio_stats::Shared;
use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn fragment_parts_publish_whole_batches_under_both_capacity_limits() {
    let ring = AtomicPcmRing::new(15); // Three logical frames, four physical slots.
    let mut producer = ring.producer();
    assert!(producer.publish_parts(&[1, 2, 3], &[4, 5, 6, 7, 8], 15));
    assert_eq!(producer.len(), 8);
    assert!(!producer.publish_parts(&[9, 10, 11], &[12, 13, 14, 15, 16], 15));
    assert!(!producer.publish_parts(&[9], &[10, 11, 12], 8));
    let mut consumer = ring.try_consumer().unwrap();
    assert!(ring.try_consumer().is_none());
    let mut short = [0xee; 6];
    assert_eq!(consumer.drain_into(&mut short), 4);
    assert_eq!(short, [1, 2, 3, 4, 0xee, 0xee]);
    assert!(producer.publish_parts(&[9], &[10, 11, 12], 8));
    let mut remainder = [0xee; 8];
    assert_eq!(consumer.drain_into(&mut remainder), 4);
    assert_eq!(remainder, [5, 6, 7, 8, 0xee, 0xee, 0xee, 0xee]);
    drop(consumer);
    let mut consumer = ring.try_consumer().unwrap();
    assert_eq!(consumer.drain_into(&mut remainder), 4);
    assert_eq!(&remainder[..4], &[9, 10, 11, 12]);
    drop(consumer);
    assert!(ring.is_empty());
    assert!(!AtomicPcmRing::new(3)
        .producer()
        .publish_parts(&[1, 2, 3, 4], &[], 4));
}

#[test]
fn wrapping_counters_keep_physical_slots_and_logical_capacity_in_order() {
    let ring = AtomicPcmRing::new(12);
    ring.head.store(usize::MAX - 1, Ordering::Relaxed);
    ring.tail.store(usize::MAX - 1, Ordering::Relaxed);
    let mut producer = ring.producer();
    assert!(producer.publish_parts(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], &[], 12));
    assert_eq!(ring.head.load(Ordering::Relaxed), 1);
    let mut consumer = ring.try_consumer().unwrap();
    let mut first = [0; 8];
    assert_eq!(consumer.drain_into(&mut first), 8);
    assert_eq!(first, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert!(producer.publish_parts(&[13, 14, 15, 16], &[17, 18, 19, 20], 12));
    let mut final_old = [0; 4];
    assert_eq!(consumer.drain_into(&mut final_old), 4);
    assert_eq!(final_old, [9, 10, 11, 12]);
    drop(consumer);
    assert_eq!(ring.len(), 8);
    let mut next = ring.try_consumer().unwrap();
    assert_eq!(next.drain_into(&mut first), 8);
    assert_eq!(first, [13, 14, 15, 16, 17, 18, 19, 20]);
}

#[test]
fn callback_drains_published_pcm_while_producer_copies_the_next_batch() {
    let shared = Shared::new(16, 0);
    let mut producer = PcmProducer::default();
    producer.write(&[1, 0, 0, 0, 2, 0, 0, 0], &shared, 16);
    std::thread::scope(|scope| {
        let (copied, wait_copy) = mpsc::sync_channel(0);
        let (commit, wait_commit) = mpsc::sync_channel(0);
        let shared_ref = &shared;
        let writer = scope.spawn(move || {
            let mut producer = shared_ref.ring.producer();
            let pending = producer
                .prepare_parts(&[3, 0, 0, 0], &[4, 0, 0, 0], 16)
                .unwrap();
            copied.send(()).unwrap();
            wait_commit.recv_timeout(Duration::from_secs(10)).unwrap();
            pending.commit();
        });
        wait_copy.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut destination = [0; 8];
        fill_and_record(&mut destination, &shared);
        assert_eq!(destination, [1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(shared.continuity.snapshot().contention_callbacks, 0);
        commit.send(()).unwrap();
        writer.join().unwrap();
        destination.fill(0);
        fill_and_record(&mut destination, &shared);
        assert_eq!(destination, [3, 0, 0, 0, 4, 0, 0, 0]);
        let continuity = shared.continuity.snapshot();
        assert_eq!((continuity.contention_callbacks, continuity.gaps), (0, 0));
    });
}
