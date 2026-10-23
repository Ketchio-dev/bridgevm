//! A capped poll must not leave its burst queued as latency (the audio
//! model's fix 3). The live producer and callback run against a two-period
//! stream shaped like T17 r92's; before the ring's 70 ms high-water mark the
//! capped poll's 100 ms stayed in the ring for the rest of playback (r92: 104).

use super::*;
use crate::hda_coreaudio::hda_coreaudio_continuity::fill_and_record;
use crate::hda_coreaudio::hda_coreaudio_prefill::PREFILL_BYTES;
use crate::hda_coreaudio::hda_coreaudio_producer::PcmProducer;
use crate::hda_coreaudio::{AUDIO_QUEUE_BUFFER_BYTES, RING_CAPACITY_BYTES};
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::hda::{HdaController, HdaPcmSink, REG_GCTL, REG_SD_BDPL};
use bridgevm_hvf::hda::{REG_SD_CBL, REG_SD_CTL, REG_SD_FMT, REG_SD_LVI};
use bridgevm_hvf::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// The live sink's producer side on a shared ring, without an AudioQueue,
/// counting every byte the controller hands it.
struct Sink {
    producer: PcmProducer,
    shared: Arc<Shared>,
    written: Arc<AtomicUsize>,
}

impl HdaPcmSink for Sink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.written.fetch_add(samples.len(), Relaxed);
        let format = (rate, channels, bits);
        let shared = &self.shared;
        self.producer
            .write_format(samples, format, shared, RING_CAPACITY_BYTES);
    }
}

/// Two 1920-byte periods, 48 kHz stereo s16, running, as r92's guest set up.
fn stream(sink: Sink) -> (HdaController, FlatGuestRam) {
    const RAM: u64 = 0x1000_0000;
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM, 0x10000);
    let (bdl, pcm) = (RAM + 0x1000, RAM + 0x2000);
    assert!(mem.write_bytes(pcm, &[0x5a; 3840]));
    for period in 0..2u64 {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&(pcm + period * 1920).to_le_bytes());
        descriptor[8..12].copy_from_slice(&1920u32.to_le_bytes());
        assert!(mem.write_bytes(bdl + period * 16, &descriptor));
    }
    for (offset, size, value) in [
        (REG_GCTL, 4, 1),
        (REG_SD_BDPL, 4, bdl),
        (REG_SD_CBL, 4, 3840),
        (REG_SD_LVI, 2, 1),
        (REG_SD_FMT, 2, 0x0011),
    ] {
        ctrl.mmio_write(offset, size, value, &mut mem);
    }
    ctrl.set_pcm_sink(Some(Box::new(sink)));
    ctrl.mmio_write(REG_SD_CTL, 1, 1 << 1, &mut mem);
    (ctrl, mem)
}

/// 0.2 ms polls for 4 s except one 216 ms stall at 2 s, r92's longest gap,
/// whose next poll the controller caps at 100 ms; a callback every 10 ms.
/// Returns the shared ring state and the bytes the controller wrote.
pub(super) fn capped_gap() -> (Arc<Shared>, usize) {
    let shared = Arc::new(Shared::new(RING_CAPACITY_BYTES, PREFILL_BYTES));
    let written = Arc::new(AtomicUsize::new(0));
    let (producer, sink_shared) = (PcmProducer::default(), Arc::clone(&shared));
    let sink_written = Arc::clone(&written);
    let (mut ctrl, mut mem) = stream(Sink {
        producer,
        shared: sink_shared,
        written: sink_written,
    });
    let start = Instant::now();
    let (stall, end) = (
        start + Duration::from_secs(2),
        start + Duration::from_secs(4),
    );
    let resume = stall + Duration::from_millis(216);
    let mut buffer = vec![0u8; AUDIO_QUEUE_BUFFER_BYTES as usize];
    let (mut poll, mut callback) = (start, start + Duration::from_micros(100));
    while poll < end {
        if callback < poll {
            fill_and_record(&mut buffer, &shared);
            callback += Duration::from_millis(10);
            continue;
        }
        if poll <= stall || poll >= resume {
            ctrl.poll(&mut mem, Some(poll));
        }
        poll += Duration::from_micros(200);
    }
    drop(ctrl);
    let written = written.load(Relaxed);
    (shared, written)
}

#[test]
fn capped_gap_does_not_hold_ring_latency() {
    let (shared, _) = capped_gap();
    let t = shared.continuity.timing.snapshot();
    // The stall drained the ring; the capped poll's burst may refill it only
    // to the mark, and the level must not stay above it.
    assert_eq!(t.ring_min_ms, 0, "{t:?}");
    assert!(t.ring_max_ms <= 70, "{t:?}");
    assert!(t.ring_final_ms <= 70, "{t:?}");
}
