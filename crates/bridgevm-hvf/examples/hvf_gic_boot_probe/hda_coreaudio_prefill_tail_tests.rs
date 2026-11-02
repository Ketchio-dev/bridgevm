//! Stopped short sounds flush and the next stream retains the start threshold.

use super::*;
use crate::hda_coreaudio::hda_coreaudio_publication::PcmQueue;
use crate::hda_coreaudio::hda_coreaudio_ring::AtomicPcmRing;

#[test]
fn a_stopped_stream_flushes_a_short_sound_and_then_re_primes() {
    let shared = Shared::new(16 * FULL, PREFILL_BYTES);
    fed(&shared, FULL / 2);
    assert_eq!(fill(&shared), 0);
    shared.continuity.note_stream_stopped();
    assert_eq!(
        fill(&shared),
        FULL / 2,
        "a stopped stream plays what it left"
    );
    fed(&shared, FULL);
    assert_eq!(
        fill(&shared),
        0,
        "the next stream waits for the threshold again"
    );
}

#[test]
fn a_zero_threshold_drains_at_once() {
    let prefill = Prefill::new(0);
    let ring = AtomicPcmRing::new(8);
    assert!(ring.producer().publish_parts(&[7; 8], &[], 8));
    let mut consumer = ring.try_consumer().unwrap();
    let mut destination = [0u8; 4];
    assert_eq!(prefill.drain(&mut consumer, &mut destination, false), 4);
    assert_eq!(destination, [7; 4]);
}
