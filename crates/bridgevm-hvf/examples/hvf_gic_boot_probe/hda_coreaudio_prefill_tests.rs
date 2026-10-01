use super::*;
use super::super::hda_coreaudio_continuity::fill_and_record;
use super::super::hda_coreaudio_stats::Shared;

const FULL: usize = AUDIO_QUEUE_BUFFER_BYTES as usize;

fn fed(shared: &Shared, bytes: usize) {
    let mut ring = shared.ring.lock().unwrap();
    ring.extend(std::iter::repeat_n(0x5a, bytes));
    shared.continuity.note_pcm_written();
}

fn fill(shared: &Shared) -> usize {
    let mut buffer = vec![0u8; FULL];
    fill_and_record(&mut buffer, shared);
    buffer.iter().filter(|&&byte| byte == 0x5a).count()
}

#[test]
fn playback_waits_for_the_threshold_and_that_silence_is_not_a_gap() {
    let shared = Shared::new(16 * FULL, PREFILL_BYTES);
    fed(&shared, PREFILL_BYTES - 1);
    assert_eq!(fill(&shared), 0, "below the threshold the buffer stays silent");
    assert_eq!(fill(&shared), 0);
    fed(&shared, 1);
    assert_eq!(fill(&shared), FULL, "the threshold starts playback");
    let snapshot = shared.continuity.snapshot();
    assert_eq!((snapshot.gaps, snapshot.underrun_callbacks), (0, 0));
}

#[test]
fn a_primed_stream_absorbs_a_producer_stall_up_to_the_reserve() {
    let shared = Shared::new(16 * FULL, PREFILL_BYTES);
    fed(&shared, PREFILL_BYTES);
    for _ in 0..PREFILL_BYTES / FULL {
        assert_eq!(fill(&shared), FULL, "the reserve covers a stall of the same length");
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
    assert_eq!(fill(&shared), FULL / 2, "no re-prime while the guest still streams");
    fed(&shared, FULL / 4);
    assert_eq!(fill(&shared), FULL / 4);
}

#[test]
fn a_stopped_stream_flushes_a_short_sound_and_then_re_primes() {
    let shared = Shared::new(16 * FULL, PREFILL_BYTES);
    fed(&shared, FULL / 2);
    assert_eq!(fill(&shared), 0);
    shared.continuity.note_stream_stopped();
    assert_eq!(fill(&shared), FULL / 2, "a stopped stream plays what it left");
    fed(&shared, FULL);
    assert_eq!(fill(&shared), 0, "the next stream waits for the threshold again");
}

#[test]
fn a_zero_threshold_drains_at_once() {
    let prefill = Prefill::new(0);
    let mut ring = VecDeque::from(vec![7u8; 8]);
    let mut destination = [0u8; 4];
    assert_eq!(prefill.drain(&mut ring, &mut destination, false), 4);
    assert_eq!(destination, [7; 4]);
}
