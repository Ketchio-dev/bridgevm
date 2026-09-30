//! The host sink learns when the guest stops a running playback stream.

use super::super::*;
use super::helpers::{write, RAM_BASE};
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct StopCountingSink {
    frames: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
}

impl HdaPcmSink for StopCountingSink {
    fn write_pcm(&mut self, samples: &[u8], _rate: u32, _channels: u8, _bits: u8) {
        self.frames.fetch_add(samples.len() / 4, Ordering::SeqCst);
    }

    fn stream_stopped(&mut self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
}

struct Stream {
    ctrl: HdaController,
    mem: FlatGuestRam,
    frames: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
}

impl Stream {
    fn new() -> Self {
        let (frames, stops) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
        ctrl.set_pcm_sink(Some(Box::new(StopCountingSink {
            frames: Arc::clone(&frames),
            stops: Arc::clone(&stops),
        })));
        let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
        let (bdl, pcm) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
        descriptor[8..12].copy_from_slice(&1920u32.to_le_bytes());
        assert!(mem.write_bytes(bdl, &descriptor));
        write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
        write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
        write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 1920);
        write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
        write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
        Self {
            ctrl,
            mem,
            frames,
            stops,
        }
    }

    fn ctl(&mut self, value: u32) {
        write(
            &mut self.ctrl,
            &mut self.mem,
            REG_SD_CTL,
            1,
            u64::from(value),
        );
    }

    fn play(&mut self) {
        self.ctl(SDCTL_RUN);
        self.ctrl
            .poll_for_duration(&mut self.mem, Duration::from_millis(1));
    }

    fn stops(&self) -> usize {
        self.stops.load(Ordering::SeqCst)
    }
}

#[test]
fn clearing_run_on_a_running_stream_reports_one_stop() {
    let mut stream = Stream::new();
    stream.ctl(0);
    assert_eq!(stream.stops(), 0, "a stream that never ran did not stop");
    stream.play();
    assert_eq!(stream.frames.load(Ordering::SeqCst), 48);
    stream.ctl(0);
    stream.ctl(0);
    assert_eq!(stream.stops(), 1);
}

#[test]
fn stream_reset_reports_a_stop_only_while_running() {
    let mut stream = Stream::new();
    stream.ctl(SDCTL_SRST);
    stream.ctl(0);
    assert_eq!(stream.stops(), 0);
    stream.play();
    stream.ctl(SDCTL_SRST);
    assert_eq!(stream.stops(), 1);
    assert_eq!(
        stream.ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN),
        0
    );
}

#[test]
fn controller_reset_reports_a_running_stream_stop_and_keeps_the_sink() {
    let mut stream = Stream::new();
    stream.play();
    write(&mut stream.ctrl, &mut stream.mem, REG_GCTL, 4, 0);
    assert_eq!(stream.stops(), 1);
    write(&mut stream.ctrl, &mut stream.mem, REG_GCTL, 4, 0);
    assert_eq!(
        stream.stops(),
        1,
        "a reset controller has no running stream"
    );
    assert!(stream.ctrl.pcm_sink.is_some());
}

#[test]
fn a_dma_error_halt_is_not_reported_as_a_guest_stop() {
    let mut stream = Stream::new();
    stream.ctl(SDCTL_RUN);
    assert!(stream
        .mem
        .write_bytes(RAM_BASE + 0x1008, &0u32.to_le_bytes()));
    stream
        .ctrl
        .poll_for_duration(&mut stream.mem, Duration::from_millis(1));
    assert_eq!(
        stream.ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN),
        0
    );
    assert_eq!(stream.stops(), 0);
}
