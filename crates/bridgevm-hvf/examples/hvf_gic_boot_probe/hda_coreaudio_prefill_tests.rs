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

#[path = "hda_coreaudio_prefill_tail_tests.rs"]
mod tail;
