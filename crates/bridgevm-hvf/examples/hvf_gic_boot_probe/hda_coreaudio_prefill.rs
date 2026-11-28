//! Start threshold for CoreAudio playback.
//!
//! HDA polling supplies guest PCM in bursts while the output callback empties
//! the ring every 10 ms. Without a reserve, the first stall after playback
//! starts leaves a partial buffer silent: gaps then come from the start of
//! each span, not from its length. The callback therefore leaves its buffer silent until the ring holds
//! `PREFILL_BYTES`, and then drains it. That silence comes before the
//! continuity span opens, so it is start latency, not a gap.
//!
//! A stopped stream flushes whatever the ring still holds, so a sound shorter
//! than the threshold still plays. Playback re-primes only after the guest
//! stopped its stream and the ring ran short, which is where a span closes;
//! an underrun inside a running stream keeps playing whatever arrives.
//!
//! The guest's PCM arrives on the Mac's clock and plays on the output device's,
//! and an underrun's silence delays everything after it, so the buffered delay
//! can only grow while a stream runs, towards the ring's half second. Above
//! `HIGH_WATER_BYTES` the oldest audio is dropped back to the start threshold,
//! as the microphone does (hda_microphone.rs). Live runs peaked at 86 ms (43
//! timing records, median 53 ms), so normal playback never reaches it.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;

use super::hda_coreaudio_atomic_ring::Consumer;
use super::AUDIO_QUEUE_BUFFER_BYTES;

/// Four callback buffers, 40 ms at 48 kHz stereo s16le.
pub(super) const PREFILL_BYTES: usize = 4 * AUDIO_QUEUE_BUFFER_BYTES as usize;
/// 120 ms at 48 kHz stereo s16le.
pub(super) const HIGH_WATER_BYTES: usize = 12 * AUDIO_QUEUE_BUFFER_BYTES as usize;

pub(super) struct Prefill {
    threshold: usize,
    primed: AtomicBool,
}

impl Prefill {
    pub(super) const fn new(threshold: usize) -> Self {
        Self {
            threshold,
            primed: AtomicBool::new(false),
        }
    }

    /// Drain the ring into one callback buffer once playback is primed, and
    /// return the PCM bytes copied. Callback thread only, under the consumer guard.
    pub(in crate::hda_coreaudio) fn drain(
        &self,
        ring: &mut Consumer<'_>,
        destination: &mut [u8],
        stream_idle: bool,
    ) -> usize {
        let flush = stream_idle && !ring.is_empty();
        if !self.primed.load(Relaxed) && ring.len() < self.threshold && !flush {
            return 0;
        }
        if !stream_idle && ring.len() > HIGH_WATER_BYTES {
            ring.discard(ring.len() - self.threshold);
        }
        let pcm_bytes = ring.drain_into(destination);
        self.primed
            .store(!(stream_idle && pcm_bytes < destination.len()), Relaxed);
        pcm_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::super::hda_coreaudio_atomic_ring::{AtomicPcmRing, PcmQueue};
    use super::super::hda_coreaudio_continuity::fill_and_record;
    use super::super::hda_coreaudio_producer::PcmProducer;
    use super::super::hda_coreaudio_stats::Shared;
    use super::*;

    const FULL: usize = AUDIO_QUEUE_BUFFER_BYTES as usize;

    fn fed(shared: &Shared, bytes: usize) {
        PcmProducer::default().write(&vec![0x5a; bytes], shared, 16 * FULL);
    }

    fn fill(shared: &Shared) -> usize {
        let mut buffer = vec![0u8; FULL];
        fill_and_record(&mut buffer, shared);
        buffer.iter().filter(|&&byte| byte == 0x5a).count()
    }

    #[test]
    fn playback_waits_for_the_threshold_and_that_silence_is_not_a_gap() {
        let shared = Shared::new(16 * FULL, PREFILL_BYTES);
        fed(&shared, PREFILL_BYTES - 4);
        assert_eq!(
            fill(&shared),
            0,
            "below the threshold the buffer stays silent"
        );
        assert_eq!(fill(&shared), 0);
        fed(&shared, 4);
        assert_eq!(fill(&shared), FULL, "the threshold starts playback");
        let snapshot = shared.continuity.snapshot();
        assert_eq!((snapshot.gaps, snapshot.underrun_callbacks), (0, 0));
    }

    #[test]
    fn a_primed_stream_absorbs_a_producer_stall_up_to_the_reserve() {
        let shared = Shared::new(16 * FULL, PREFILL_BYTES);
        fed(&shared, PREFILL_BYTES);
        for _ in 0..PREFILL_BYTES / FULL {
            assert_eq!(
                fill(&shared),
                FULL,
                "the reserve covers a stall of the same length"
            );
        }
        fed(&shared, FULL);
        assert_eq!(fill(&shared), FULL);
        assert_eq!(shared.continuity.snapshot().gaps, 0);
    }

    #[test]
    fn an_underrun_inside_a_running_stream_keeps_playing_what_arrives() {
        let shared = Shared::new(16 * FULL, PREFILL_BYTES);
        fed(&shared, PREFILL_BYTES + FULL / 2);
        for _ in 0..PREFILL_BYTES / FULL {
            fill(&shared);
        }
        assert_eq!(
            fill(&shared),
            FULL / 2,
            "no re-prime while the guest still streams"
        );
        fed(&shared, FULL / 4);
        assert_eq!(fill(&shared), FULL / 4);
    }

    // Stopped short sounds flush and the next stream retains the start threshold.

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

    /// A delay that crept past the high water while the stream ran (underruns, the two clocks) is
    /// cut back to the start threshold by dropping the oldest audio: lip sync slipped towards half
    /// a second and stayed there.
    #[test]
    fn a_running_stream_buffered_past_the_high_water_drops_its_oldest_audio() {
        let prefill = Prefill::new(PREFILL_BYTES);
        let ring = AtomicPcmRing::new(64 * FULL);
        let frames = |first: u32, bytes: usize| -> Vec<u8> {
            (first..first + (bytes / 4) as u32)
                .flat_map(u32::to_le_bytes)
                .collect()
        };
        let buffered = HIGH_WATER_BYTES + 8 * FULL;
        assert!(ring
            .producer()
            .publish_parts(&frames(0, buffered), &[], 64 * FULL));
        let mut consumer = ring.try_consumer().unwrap();
        let mut destination = vec![0u8; FULL];
        assert_eq!(prefill.drain(&mut consumer, &mut destination, false), FULL);
        let first = u32::from_le_bytes(destination[..4].try_into().unwrap());
        assert_eq!(
            first as usize,
            (buffered - PREFILL_BYTES) / 4,
            "plays from the newest 40 ms"
        );
        assert_eq!(consumer.len(), PREFILL_BYTES - FULL);
        // At the high water itself, nothing is dropped.
        drop(consumer);
        let ring = AtomicPcmRing::new(64 * FULL);
        assert!(ring
            .producer()
            .publish_parts(&frames(0, HIGH_WATER_BYTES), &[], 64 * FULL));
        let mut consumer = ring.try_consumer().unwrap();
        assert_eq!(prefill.drain(&mut consumer, &mut destination, false), FULL);
        assert_eq!(u32::from_le_bytes(destination[..4].try_into().unwrap()), 0);
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
}
