use super::super::hda_coreaudio_continuity::fill_and_record;
use super::super::hda_coreaudio_prefill::PREFILL_BYTES;
use super::super::hda_coreaudio_producer::PcmProducer;
use super::super::hda_coreaudio_stats::Shared;
use super::super::AUDIO_QUEUE_BUFFER_BYTES;
use super::*;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn callback_retains_partial_stereo_frame_until_next_dma_fragment() {
    let shared = Shared::new(16 * AUDIO_QUEUE_BUFFER_BYTES as usize, PREFILL_BYTES);
    let mut producer = PcmProducer::default();
    producer.write(&vec![0x5a; PREFILL_BYTES], &shared, 16 * PREFILL_BYTES);
    for _ in 0..PREFILL_BYTES / AUDIO_QUEUE_BUFFER_BYTES as usize {
        let mut warmup = vec![0; AUDIO_QUEUE_BUFFER_BYTES as usize];
        fill_and_record(&mut warmup, &shared);
        assert!(warmup.iter().all(|&byte| byte == 0x5a));
    }
    producer.write(&[1, 2, 3, 4, 5, 6], &shared, 16 * PREFILL_BYTES);
    let mut first = [0; 8];
    fill_and_record(&mut first, &shared);
    assert_eq!(first, [1, 2, 3, 4, 0, 0, 0, 0]);
    assert!(shared.ring.lock().unwrap().is_empty());
    producer.write(&[7, 8, 9, 10, 11, 12], &shared, 16 * PREFILL_BYTES);
    let mut second = [0; 8];
    fill_and_record(&mut second, &shared);
    assert_eq!(second, [5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(
        shared.frames_rendered.load(Relaxed),
        (PREFILL_BYTES / 4 + 3) as u64
    );
}

#[test]
fn terminal_fragments_pad_once_and_are_not_complete_guest_frames() {
    for count in 1..4 {
        let shared = Shared::new(32, PREFILL_BYTES);
        let mut producer = PcmProducer::default();
        producer.write(&[1, 2, 3][..count], &shared, 32);
        producer.finish(&shared, 32);
        producer.finish(&shared, 32);
        assert!(shared.continuity.stream_idle());
        assert_eq!(shared.continuity.snapshot().stream_stops, 0);
        assert_eq!(shared.frames_rendered.load(Relaxed), 0);
        let mut output = [0; 8];
        fill_and_record(&mut output, &shared);
        assert_eq!(&output[..count], &[1, 2, 3][..count]);
        assert!(output[count..].iter().all(|&b| b == 0));
        assert!(shared.ring.lock().unwrap().is_empty());
    }
}

#[test]
fn reset_before_callback_preserves_complete_frames_but_separates_generations() {
    let shared = Shared::new(32, 0);
    let mut producer = PcmProducer::default();
    producer.write(&[1, 2, 3, 4, 5, 6], &shared, 32);
    producer.finish(&shared, 32);
    producer.write(&[7, 8, 9, 10], &shared, 32);
    let mut output = [0; 12];
    fill_and_record(&mut output, &shared);
    assert_eq!(output, [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10]);
    assert_eq!(shared.frames_rendered.load(Relaxed), 2);
}

#[test]
fn run_pause_preserves_fragment_for_cursor_resuming_write() {
    let shared = Shared::new(32, PREFILL_BYTES);
    let mut producer = PcmProducer::default();
    producer.write(&[1, 2], &shared, 32);
    shared.continuity.note_stream_stopped();
    let mut paused = [0; 4];
    fill_and_record(&mut paused, &shared);
    assert_eq!(paused, [0; 4]);
    producer.write(&[3, 4], &shared, 32);
    shared.continuity.note_stream_stopped();
    let mut resumed = [0; 4];
    fill_and_record(&mut resumed, &shared);
    assert_eq!(resumed, [1, 2, 3, 4]);
    assert_eq!(shared.frames_rendered.load(Relaxed), 1);
}

#[test]
fn capacity_loss_drops_whole_frames_and_retains_source_phase() {
    let mut frames = PcmFrames::default();
    let mut ring = VecDeque::from([90, 91, 92, 93]);
    assert_eq!(frames.write(&[1, 2], &mut ring, 4), Publication::default());
    assert_eq!(
        frames.write(&[3, 4, 5], &mut ring, 4),
        Publication {
            guest_frames: 0,
            dropped_bytes: 4,
        }
    );
    ring.clear();
    assert_eq!(
        frames.write(&[6, 7, 8], &mut ring, 4),
        Publication {
            guest_frames: 1,
            dropped_bytes: 0,
        }
    );
    assert_eq!(ring, [5, 6, 7, 8]);
}

#[test]
fn terminal_capacity_loss_counts_only_actual_source_bytes_once() {
    for count in 1..4 {
        let mut frames = PcmFrames::default();
        let mut ring = VecDeque::from([90; 4]);
        frames.write(&[1, 2, 3][..count], &mut ring, 4);
        assert_eq!(
            frames.finish(&mut ring, 4),
            Publication {
                guest_frames: 0,
                dropped_bytes: count,
            }
        );
        assert_eq!(frames.finish(&mut ring, 4), Publication::default());
        assert_eq!(ring, [90; 4]);
    }
}

#[test]
fn every_fragment_split_preserves_complete_byte_stream() {
    let source: Vec<u8> = (0..64).collect();
    for split in 1..64 {
        let mut frames = PcmFrames::default();
        let mut ring = VecDeque::new();
        let first = frames.write(&source[..split], &mut ring, 64);
        assert_eq!(ring.len() % 4, 0);
        let second = frames.write(&source[split..], &mut ring, 64);
        assert_eq!(first.guest_frames + second.guest_frames, 16);
        assert_eq!(ring.iter().copied().collect::<Vec<_>>(), source);
    }
}
