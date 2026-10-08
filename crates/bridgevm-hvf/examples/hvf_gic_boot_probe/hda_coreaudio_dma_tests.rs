//! Real HDA DMA → framed producer → callback, without creating an AudioQueue.
use super::super::hda_coreaudio_continuity::fill_and_record;
use super::*;
use bridgevm_hvf::{fwcfg::GuestMemoryMut, hda::*, platform_virt::FlatGuestRam};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

const RUN: u64 = 1 << 1;

struct Sink {
    producer: PcmProducer,
    shared: Arc<Shared>,
    callbacks: Arc<Mutex<Vec<[u8; 8]>>>,
}

impl Sink {
    fn callback(&self) {
        let mut bytes = [0; 8];
        fill_and_record(&mut bytes, &self.shared);
        self.callbacks.lock().unwrap().push(bytes);
    }
}

impl HdaPcmSink for Sink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.producer
            .write_format(samples, (rate, channels, bits), &self.shared, 32);
        self.callback();
    }
    fn stream_stopped(&mut self) {
        self.shared.continuity.note_stream_stopped();
    }
    fn stream_reset(&mut self) {
        self.producer.finish(&self.shared, 32);
        self.callback();
    }
}

#[test]
fn dma_fragments_can_interleave_callbacks_without_splitting_stereo_frames() {
    run(false);
}

#[test]
fn dma_fault_then_controller_reset_finishes_tail_without_inventing_stop() {
    run(true);
}

fn run(fault: bool) {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(0x1000_0000, 0x10000);
    let (bdl, pcm) = (0x1000_1000u64, 0x1000_2000u64);
    assert!(mem.write_bytes(pcm, &[1, 2, 3, 4, 5, 6, 7, 8]));
    for (index, offset, length) in [(0u64, 0u64, 6u32), (1, 6, if fault { 0 } else { 2 })] {
        let mut descriptor = [0; 16];
        descriptor[..8].copy_from_slice(&(pcm + offset).to_le_bytes());
        descriptor[8..12].copy_from_slice(&length.to_le_bytes());
        assert!(mem.write_bytes(bdl + index * 16, &descriptor));
    }
    for (offset, size, value) in [
        (REG_GCTL, 4, 1),
        (REG_SD_BDPL, 4, bdl),
        (REG_SD_CBL, 4, 8),
        (REG_SD_LVI, 2, 1),
        (REG_SD_FMT, 2, 0x0011),
    ] {
        ctrl.mmio_write(offset, size, value, &mut mem);
    }
    let shared = Arc::new(Shared::new(32, 0));
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    ctrl.set_pcm_sink(Some(Box::new(Sink {
        producer: PcmProducer::default(),
        shared: Arc::clone(&shared),
        callbacks: Arc::clone(&callbacks),
    })));
    ctrl.mmio_write(REG_SD_CTL, 1, RUN, &mut mem);
    ctrl.poll_for_duration(&mut mem, Duration::from_micros(42));
    if fault {
        assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1) & RUN, 0);
        ctrl.mmio_write(REG_GCTL, 4, 0, &mut mem);
        assert_eq!(
            *callbacks.lock().unwrap(),
            [[1, 2, 3, 4, 0, 0, 0, 0], [5, 6, 0, 0, 0, 0, 0, 0]]
        );
        assert_eq!(shared.frames_rendered.load(Ordering::Relaxed), 1);
        assert_eq!(shared.continuity.snapshot().stream_stops, 0);
        assert!(shared.continuity.stream_idle());
    } else {
        assert_eq!(
            *callbacks.lock().unwrap(),
            [[1, 2, 3, 4, 0, 0, 0, 0], [5, 6, 7, 8, 0, 0, 0, 0]]
        );
        assert_eq!(shared.frames_rendered.load(Ordering::Relaxed), 2);
    }
}
