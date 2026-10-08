//! PCM generation boundaries are independent of guest-stop accounting.

use super::super::*;
use super::helpers::{RecordedPcm, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Pcm(RecordedPcm),
    Stop,
    Reset,
}

struct RecordingSink(Arc<Mutex<Vec<Event>>>);

impl HdaPcmSink for RecordingSink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.0.lock().unwrap().push(Event::Pcm(RecordedPcm {
            samples: samples.to_vec(),
            rate,
            channels,
            bits,
        }));
    }

    fn stream_stopped(&mut self) {
        self.0.lock().unwrap().push(Event::Stop);
    }

    fn stream_reset(&mut self) {
        self.0.lock().unwrap().push(Event::Reset);
    }
}

struct Stream {
    ctrl: HdaController,
    mem: FlatGuestRam,
    events: Arc<Mutex<Vec<Event>>>,
}

impl Stream {
    fn new() -> Self {
        let mut stream = Self {
            ctrl: HdaController::with_pcm_output_path::<&Path>(None),
            mem: FlatGuestRam::new(RAM_BASE, 0x10000),
            events: Arc::new(Mutex::new(Vec::new())),
        };
        let (bdl, pcm) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
        let samples: Vec<u8> = (0..384).map(|i| i as u8).collect();
        assert!(stream.mem.write_bytes(pcm, &samples));
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
        descriptor[8..12].copy_from_slice(&384u32.to_le_bytes());
        descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
        assert!(stream.mem.write_bytes(bdl, &descriptor));
        stream.write(REG_GCTL, 4, 1);
        stream.write(REG_SD_BDPL, 4, bdl);
        stream.write(REG_SD_CBL, 4, 384);
        stream.write(REG_SD_FMT, 2, 0x0011);
        stream
            .ctrl
            .set_pcm_sink(Some(Box::new(RecordingSink(Arc::clone(&stream.events)))));
        stream
    }

    fn write(&mut self, offset: u64, size: u8, value: u64) {
        self.ctrl.mmio_write(offset, size, value, &mut self.mem);
    }

    fn ctl(&mut self, value: u32) {
        self.write(REG_SD_CTL, 1, u64::from(value));
    }

    fn poll(&mut self) {
        self.ctrl
            .poll_for_duration(&mut self.mem, Duration::from_micros(500));
    }

    fn events(&self) -> Vec<Event> {
        self.events.lock().unwrap().clone()
    }
}

fn pcm(range: std::ops::Range<usize>) -> Event {
    Event::Pcm(RecordedPcm {
        samples: range.map(|i| i as u8).collect(),
        rate: 48_000,
        channels: 2,
        bits: 16,
    })
}

#[test]
fn every_controller_and_sd_reset_notifies_even_when_run_is_already_clear() {
    for (offset, value) in [(REG_GCTL, 0), (REG_SD_CTL, u64::from(SDCTL_SRST))] {
        let mut stream = Stream::new();
        for resets in 1..=3 {
            stream.write(offset, 1, value);
            assert_eq!(stream.events(), vec![Event::Reset; resets]);
            assert_eq!(
                stream.ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN),
                0
            );
        }
        assert!(stream.ctrl.pcm_sink.is_some());
    }
}

#[test]
fn running_resets_report_one_stop_and_a_generation_boundary() {
    for (offset, value) in [(REG_GCTL, 0), (REG_SD_CTL, u64::from(SDCTL_SRST))] {
        let mut stream = Stream::new();
        stream.ctl(SDCTL_RUN);
        stream.poll();
        stream.write(offset, 1, value);
        stream.write(offset, 1, value);
        assert_eq!(
            stream.events(),
            vec![pcm(0..96), Event::Stop, Event::Reset, Event::Reset]
        );
    }
}

#[test]
fn run_pause_resume_preserves_pcm_without_a_generation_boundary() {
    let mut stream = Stream::new();
    stream.ctl(0);
    stream.ctl(SDCTL_RUN);
    stream.poll();
    stream.ctl(0);
    stream.ctl(0);
    stream.poll();
    assert_eq!(stream.events(), vec![pcm(0..96), Event::Stop]);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_LPIB, 4), 96);
    stream.ctl(SDCTL_RUN);
    stream.poll();
    assert_eq!(stream.events(), vec![pcm(0..96), Event::Stop, pcm(96..192)]);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_LPIB, 4), 192);
}

#[test]
fn effective_stopped_format_changes_notify_including_partial_writes() {
    let mut stream = Stream::new();
    stream.write(REG_SD_FMT, 2, 0x4011);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x4011);
    assert_eq!(stream.events(), vec![Event::Reset]);
    stream.write(REG_SD_FMT, 1, 0x10);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x4010);
    assert_eq!(stream.events(), vec![Event::Reset; 2]);
    stream.write(REG_SD_FMT + 1, 1, 0);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x0010);
    assert_eq!(stream.events(), vec![Event::Reset; 3]);
}

#[test]
fn unchanged_and_masked_format_writes_do_not_notify() {
    let mut stream = Stream::new();
    stream.write(REG_SD_FMT, 2, 0x0011);
    stream.write(REG_SD_FMT, 2, 0x8011);
    stream.write(REG_SD_FMT, 1, 0x11);
    stream.write(REG_SD_FMT + 1, 1, 0x80);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x0011);
    assert!(stream.events().is_empty());
}

#[test]
fn running_format_writes_are_rejected_without_reset_until_stopped() {
    let mut stream = Stream::new();
    stream.ctl(SDCTL_RUN);
    stream.write(REG_SD_FMT, 2, 0x4010);
    stream.write(REG_SD_FMT, 1, 0x10);
    stream.write(REG_SD_FMT + 1, 1, 0x40);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x0011);
    assert!(stream.events().is_empty());
    stream.poll();
    stream.ctl(0);
    stream.write(REG_SD_FMT, 2, 0x4010);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_FMT, 2), 0x4010);
    assert_eq!(stream.events(), vec![pcm(0..96), Event::Stop, Event::Reset]);
}

#[test]
fn six_byte_dma_then_error_resets_generation_without_a_guest_stop() {
    for (offset, value) in [(REG_GCTL, 0), (REG_SD_CTL, u64::from(SDCTL_SRST))] {
        let mut stream = Stream::new();
        // The first descriptor ends between stereo frames; the next is invalid.
        assert!(stream
            .mem
            .write_bytes(RAM_BASE + 0x1008, &6u32.to_le_bytes()));
        stream.write(REG_SD_LVI, 2, 1);
        let position = RAM_BASE + 0x3000;
        stream.write(REG_DPLBASE, 4, position | 1);
        stream.write(REG_INTCTL, 4, u64::from(INTCTL_GIE | INTCTL_STREAM0));
        stream.ctl(SDCTL_RUN | SDCTL_IOCE | SDCTL_DEIE);
        stream.poll();

        assert_eq!(stream.events(), vec![pcm(0..6)]);
        assert_eq!(
            stream.ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN),
            0
        );
        assert_eq!(stream.ctrl.mmio_read(REG_SD_LPIB, 4), 6);
        assert_eq!(
            stream.mem.read_bytes(position, 4).unwrap(),
            6u32.to_le_bytes()
        );
        assert_eq!(
            (stream.ctrl.stream.bdl_index, stream.ctrl.stream.bdl_offset),
            (1, 0)
        );
        assert_eq!(
            stream.ctrl.mmio_read(REG_SD_STS, 1),
            u64::from(SDSTS_BCIS | SDSTS_DESE)
        );
        assert!(stream.ctrl.interrupt_level());
        stream.poll();
        assert_eq!(stream.events(), vec![pcm(0..6)]);

        stream.write(offset, 1, value);
        assert_eq!(stream.events(), vec![pcm(0..6), Event::Reset]);
        assert_eq!(stream.ctrl.mmio_read(REG_SD_LPIB, 4), 0);
        assert_eq!(stream.ctrl.mmio_read(REG_SD_STS, 1), 0);
        assert!(!stream.ctrl.interrupt_level());
    }
}
