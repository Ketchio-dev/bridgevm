//! A5 in the capped-gap case: "Guest PCM reaches the host CoreAudio ring with
//! frames_rendered>0 AND drops==0". With the live producer and callback fill,
//! a 216 ms mid-stream stall (r92's longest gap, which the controller caps at
//! 100 ms) and then normal polls must deliver every polled byte to the ring.
//! The ring's 70 ms high-water mark trimmed part of the capped poll's burst as
//! ring-full drops, T17 r94's audio stage failed on them, and the mark is
//! withdrawn.

use crate::hda_coreaudio::hda_coreaudio_continuity::fill_and_record;
use crate::hda_coreaudio::hda_coreaudio_prefill::PREFILL_BYTES;
use crate::hda_coreaudio::hda_coreaudio_producer::PcmProducer;
use crate::hda_coreaudio::hda_coreaudio_stats::Shared;
use crate::hda_coreaudio::{AUDIO_QUEUE_BUFFER_BYTES, RING_CAPACITY_BYTES};
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::hda::{HdaController, HdaPcmSink, REG_GCTL, REG_SD_BDPL};
use bridgevm_hvf::hda::{REG_SD_CBL, REG_SD_CTL, REG_SD_FMT, REG_SD_LVI};
use bridgevm_hvf::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// One counter of the controller's `hda poll:` record.
fn poll_field(ctrl: &HdaController, name: &str) -> u64 {
    let records = ctrl.diagnostic_records();
    let record = records.iter().find(|r| r.starts_with("hda poll:")).unwrap();
    let key = format!("{name}=");
    let value = record.split(' ').find_map(|token| token.strip_prefix(&key));
    value.unwrap().parse().unwrap()
}

#[test]
fn a_capped_gap_delivers_every_polled_byte_to_the_ring_without_drops() {
    let shared = Arc::new(Shared::new(RING_CAPACITY_BYTES, PREFILL_BYTES));
    let written = Arc::new(AtomicUsize::new(0));
    let (mut ctrl, mut mem) = stream(Sink {
        producer: PcmProducer::default(),
        shared: Arc::clone(&shared),
        written: Arc::clone(&written),
    });
    // 0.2 ms polls for 4 s except one 216 ms stall at 2 s; a callback every 10 ms.
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
    let capped = (
        poll_field(&ctrl, "capped_polls"),
        poll_field(&ctrl, "capped_lost_ms"),
    );
    assert_eq!(capped, (1, 116), "the stall is one capped poll");
    drop(ctrl);
    // Every polled byte reached the ring, but for a partial frame the
    // producer still holds for the next write.
    let written = written.load(Relaxed);
    let rendered = shared.frames_rendered.load(Relaxed) as usize * 4;
    // drops, dropped_bytes, format_drops and ring_full_drops, as the stats
    // record prints them.
    let drops = [
        &shared.dropped_writes,
        &shared.dropped_bytes,
        &shared.format_drops,
        &shared.ring_full_drops,
    ]
    .map(|counter| counter.load(Relaxed));
    let private = written.checked_sub(rendered);
    assert!(
        rendered > 0 && matches!(private, Some(0..=3)),
        "written={written} rendered={rendered} drops={drops:?}"
    );
    assert_eq!(drops, [0; 4]);
}
