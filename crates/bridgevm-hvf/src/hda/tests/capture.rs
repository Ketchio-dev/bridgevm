//! The capture stream (SD0) and the input-first stream layout.

use super::super::*;
use super::helpers::{write, RAM_BASE};
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BDL: u64 = RAM_BASE + 0x1000;
const BUFFER: u64 = RAM_BASE + 0x2000;
const POSITION: u64 = RAM_BASE + 0x3000;
const ENTRY: u32 = 192; // 1 ms of 48 kHz stereo s16le.

#[derive(Default)]
struct Log {
    events: Vec<&'static str>,
    next: u8,
}

/// Counts bytes up from 1 and supplies at most `limit` bytes per read.
struct Counting {
    log: Arc<Mutex<Log>>,
    limit: usize,
}

impl HdaPcmSource for Counting {
    fn read_pcm(&mut self, out: &mut [u8], rate: u32, channels: u8, bits: u8) -> usize {
        assert_eq!((rate, channels, bits), (48_000, 2, 16));
        let mut log = self.log.lock().unwrap();
        let filled = out.len().min(self.limit);
        for byte in &mut out[..filled] {
            log.next = log.next.wrapping_add(1);
            *byte = log.next;
        }
        filled
    }
    fn stream_started(&mut self) {
        self.log.lock().unwrap().events.push("started");
    }
    fn stream_stopped(&mut self) {
        self.log.lock().unwrap().events.push("stopped");
    }
}

struct Capture {
    ctrl: HdaController,
    mem: FlatGuestRam,
    log: Arc<Mutex<Log>>,
}

impl Capture {
    /// Two 192-byte entries, the first with IOC, in a 384-byte cyclic buffer.
    fn new(limit: Option<usize>) -> Self {
        let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
        let log = Arc::new(Mutex::new(Log::default()));
        if let Some(limit) = limit {
            let source = Counting {
                log: Arc::clone(&log),
                limit,
            };
            ctrl.set_pcm_source(Some(Box::new(source)));
        }
        let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
        assert!(mem.write_bytes(BUFFER, &[0xaa; 2 * ENTRY as usize]));
        for (index, flags) in [(0u64, BDL_IOC), (1, 0)] {
            let mut descriptor = [0u8; 16];
            let address = BUFFER + index * u64::from(ENTRY);
            descriptor[..8].copy_from_slice(&address.to_le_bytes());
            descriptor[8..12].copy_from_slice(&ENTRY.to_le_bytes());
            descriptor[12..16].copy_from_slice(&flags.to_le_bytes());
            assert!(mem.write_bytes(BDL + index * 16, &descriptor));
        }
        let mut capture = Self { ctrl, mem, log };
        capture.write(REG_GCTL, 4, 1);
        capture.write(REG_DPLBASE, 4, POSITION | 1);
        capture.write(REG_CAPTURE_SD_BDPL, 4, BDL);
        capture.write(REG_CAPTURE_SD_CBL, 4, u64::from(2 * ENTRY));
        capture.write(REG_CAPTURE_SD_LVI, 2, 1);
        capture.write(REG_CAPTURE_SD_FMT, 2, 0x0011);
        capture.write(REG_INTCTL, 4, u64::from(INTCTL_GIE | INTCTL_CAPTURE_STREAM));
        capture
    }

    fn write(&mut self, offset: u64, size: u8, value: u64) {
        write(&mut self.ctrl, &mut self.mem, offset, size, value);
    }

    fn run(&mut self) {
        self.write(
            REG_CAPTURE_SD_CTL,
            1,
            u64::from(SDCTL_RUN | SDCTL_IOCE | SDCTL_DEIE),
        );
    }

    fn poll_ms(&mut self, ms: u64) {
        self.ctrl
            .poll_capture_for_duration(&mut self.mem, Duration::from_millis(ms));
    }

    fn buffer(&self) -> Vec<u8> {
        self.mem.read_bytes(BUFFER, 2 * ENTRY as usize).unwrap()
    }

    fn position(&self, entry: u64) -> u32 {
        let bytes = self.mem.read_bytes(POSITION + entry, 4).unwrap();
        u32::from_le_bytes(bytes.try_into().unwrap())
    }

    fn events(&self) -> Vec<&'static str> {
        self.log.lock().unwrap().events.clone()
    }
}

/// Windows' HD Audio bus driver budgets link bandwidth for each DMA engine from OUTPAY and
/// INPAY (16-bit words per 48 kHz frame). With INPAY 0 no capture stream fits, and the live
/// check's exclusive 48 kHz stereo 16-bit capture failed with AUDCLNT_E_UNSUPPORTED_FORMAT.
#[test]
fn the_link_advertises_input_bandwidth_for_capture() {
    let ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let (outpay, inpay) = (ctrl.mmio_read(0x04, 2), ctrl.mmio_read(0x06, 2));
    assert_eq!(
        (outpay, inpay),
        (0x3c, 0x1d),
        "ICH6 (and QEMU) payload capabilities"
    );
}

#[test]
fn input_streams_come_first_and_each_lpib_has_its_alias() {
    let mut capture = Capture::new(Some(usize::MAX));
    assert_eq!(capture.ctrl.mmio_read(REG_GCAP, 2), 0x1101);
    assert_eq!((REG_CAPTURE_SD_CTL, REG_SD_CTL), (0x80, 0xa0));
    assert_eq!(capture.ctrl.mmio_read(REG_CAPTURE_SD_CBL, 4), 384);
    assert_eq!(capture.ctrl.mmio_read(REG_SD_CBL, 4), 0, "playback is SD1");
    capture.write(REG_SD_CBL, 4, 4096);
    assert_eq!(capture.ctrl.mmio_read(0xa8, 4), 4096);
    capture.run();
    capture.poll_ms(1);
    assert_eq!(capture.ctrl.mmio_read(REG_CAPTURE_SD_LPIB, 4), 192);
    assert_eq!(capture.ctrl.mmio_read(0x2084, 4), 192, "capture alias");
    assert_eq!(capture.ctrl.mmio_read(0x20a4, 4), 0, "playback alias");
}

#[test]
fn captured_bytes_land_in_guest_memory_before_the_position_moves() {
    let mut capture = Capture::new(Some(usize::MAX));
    capture.run();
    assert!(capture.ctrl.stream_activity().running());
    capture.poll_ms(1);
    let expected: Vec<u8> = (1..=192).map(|value| value as u8).collect();
    let buffer = capture.buffer();
    assert_eq!(&buffer[..192], &expected[..]);
    assert_eq!(
        &buffer[192..],
        &[0xaa; 192][..],
        "nothing past the position"
    );
    assert_eq!(capture.position(CAPTURE_POSITION_ENTRY), 192);
    assert_eq!(capture.position(PLAYBACK_POSITION_ENTRY), 0);
    // The first entry completed with IOC: stream 0's interrupt, not stream 1's.
    assert_eq!(
        capture.ctrl.mmio_read(REG_CAPTURE_SD_STS, 1),
        u64::from(SDSTS_BCIS)
    );
    assert_eq!(
        capture.ctrl.mmio_read(REG_INTSTS, 4),
        u64::from(INTSTS_GIS | INTCTL_CAPTURE_STREAM)
    );
    assert!(capture.ctrl.interrupt_level());
    capture.write(REG_CAPTURE_SD_STS, 1, u64::from(SDSTS_BCIS));
    assert!(!capture.ctrl.interrupt_level());
    // The second entry, then the wrap back to the first.
    capture.poll_ms(1);
    assert_eq!(
        capture.ctrl.mmio_read(REG_CAPTURE_SD_LPIB, 4),
        0,
        "wrapped at CBL"
    );
    assert_eq!(
        capture.ctrl.mmio_read(REG_CAPTURE_SD_STS, 1),
        0,
        "no IOC on entry 1"
    );
    capture.poll_ms(1);
    let wrapped: Vec<u8> = (385..=576).map(|value| value as u8).collect();
    assert_eq!(&capture.buffer()[..192], &wrapped[..]);
    assert_eq!(capture.position(CAPTURE_POSITION_ENTRY), 192);
}

#[test]
fn silence_fills_what_the_source_cannot() {
    let mut short = Capture::new(Some(100));
    short.run();
    short.poll_ms(1);
    let buffer = short.buffer();
    let supplied: Vec<u8> = (1..=100).map(|value| value as u8).collect();
    assert_eq!(&buffer[..100], &supplied[..]);
    assert_eq!(&buffer[100..192], &[0; 92][..]);
    let mut none = Capture::new(None);
    none.run();
    none.poll_ms(1);
    assert_eq!(&none.buffer()[..192], &[0; 192][..]);
    assert_eq!(none.ctrl.mmio_read(REG_CAPTURE_SD_LPIB, 4), 192);
}

#[test]
fn the_source_hears_every_start_and_stop() {
    let mut capture = Capture::new(Some(usize::MAX));
    capture.run();
    capture.write(REG_CAPTURE_SD_CTL, 1, 0);
    capture.run();
    capture.write(REG_CAPTURE_SD_CTL, 1, u64::from(SDCTL_SRST));
    capture.write(REG_CAPTURE_SD_CTL, 1, 0);
    capture.run();
    capture.write(REG_GCTL, 4, 0); // controller reset
    assert_eq!(
        capture.events(),
        ["started", "stopped", "started", "stopped", "started", "stopped"]
    );
    assert!(!capture.ctrl.stream_activity().running());
    // The reset kept the host's source: a new stream still reads from it.
    capture.write(REG_GCTL, 4, 1);
    capture.write(REG_CAPTURE_SD_BDPL, 4, BDL);
    capture.write(REG_CAPTURE_SD_CBL, 4, u64::from(2 * ENTRY));
    capture.write(REG_CAPTURE_SD_LVI, 2, 1);
    capture.write(REG_CAPTURE_SD_FMT, 2, 0x0011);
    capture.run();
    capture.poll_ms(1);
    assert_ne!(&capture.buffer()[..192], &[0; 192][..]);
}

#[test]
fn a_bad_descriptor_halts_capture_with_dese() {
    let mut capture = Capture::new(Some(usize::MAX));
    capture.write(REG_CAPTURE_SD_BDPL, 4, 0x7fff_ff80);
    capture.run();
    capture.poll_ms(1);
    let sts = capture.ctrl.mmio_read(REG_CAPTURE_SD_STS, 1);
    assert_eq!(sts & u64::from(SDSTS_DESE), u64::from(SDSTS_DESE));
    assert_eq!(
        capture.ctrl.mmio_read(REG_CAPTURE_SD_CTL, 1) & u64::from(SDCTL_RUN),
        0
    );
    assert_eq!(capture.events(), ["started", "stopped"]);
    assert!(!capture.ctrl.stream_activity().running());
}

#[test]
fn either_running_stream_keeps_the_service_tick_awake() {
    let mut capture = Capture::new(Some(usize::MAX));
    capture.run();
    capture.write(REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    capture.write(REG_SD_CTL, 1, 0);
    assert!(
        capture.ctrl.stream_activity().running(),
        "capture still runs"
    );
    capture.write(REG_CAPTURE_SD_CTL, 1, 0);
    assert!(!capture.ctrl.stream_activity().running());
}

#[test]
fn the_codec_has_a_microphone_path_into_the_adc() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let get = |nid: u32, parameter: u32| (nid << 20) | 0x000f_0000 | parameter;
    assert_eq!(ctrl.codec_verb(get(1, 0x04)), 0x0002_0004, "nodes 2 to 5");
    assert_eq!(
        ctrl.codec_verb(get(4, 0x09)) >> 20 & 0xf,
        1,
        "node 4 is an input converter"
    );
    assert_eq!(ctrl.codec_verb(get(4, 0x0a)), 0x0002_0040, "16-bit, 48 kHz");
    assert_eq!(ctrl.codec_verb(get(4, 0x0e)), 1);
    assert_eq!(
        ctrl.codec_verb(0x004f_0200),
        u32::from(CODEC_MIC),
        "ADC connects to pin 5"
    );
    assert_eq!(
        ctrl.codec_verb(get(5, 0x09)) >> 20 & 0xf,
        4,
        "node 5 is a pin"
    );
    assert_eq!(ctrl.codec_verb(get(5, 0x0c)), 0x20, "input capable");
    assert_eq!(
        ctrl.codec_verb(0x005f_1c00),
        0x90a7_0120,
        "fixed internal mic"
    );
    // Converter format and stream/channel, and the pin's input enable.
    assert_eq!(ctrl.codec_verb(0x0042_0031), 0);
    assert_eq!(ctrl.codec_verb(0x004a_0000), 0x0031);
    assert_eq!(ctrl.codec_verb(0x0047_0610), 0);
    assert_eq!(ctrl.codec_verb(0x004f_0600), 0x10);
    assert_eq!(ctrl.codec_verb(0x005f_0700), 0x20);
    assert_eq!(ctrl.codec_verb(0x0057_0700), 0);
    assert_eq!(ctrl.codec_verb(0x005f_0700), 0);
    // The output path is unchanged.
    assert_eq!(ctrl.codec_verb(0x002a_0000), 0x0011);
    assert_eq!(ctrl.codec_verb(0x003f_0200), u32::from(CODEC_DAC));
}

#[test]
fn a_capture_ioc_gets_its_own_msi_while_playback_is_pending() {
    let mut capture = Capture::new(Some(usize::MAX));
    capture.write(
        REG_INTCTL,
        4,
        u64::from(INTCTL_GIE | INTCTL_CAPTURE_STREAM | INTCTL_PLAYBACK_STREAM),
    );
    let mut messages = Vec::new();
    let mut drain = |ctrl: &mut HdaController| {
        messages.clear();
        ctrl.drain_pending_msi_into(true, 0xfee0_0000, 0x41, &mut messages);
        messages.len()
    };
    // Playback's IOC is pending: one message.
    capture.ctrl.stream.ctl = SDCTL_RUN | SDCTL_IOCE;
    capture.ctrl.stream.sts = SDSTS_BCIS;
    assert_eq!(drain(&mut capture.ctrl), 1);
    assert_eq!(drain(&mut capture.ctrl), 0, "no new source");
    // Capture's IOC rises before the guest clears playback's: another one.
    capture.run();
    capture.poll_ms(1);
    assert_eq!(drain(&mut capture.ctrl), 1);
    // The guest clears playback's; capture's still pending raises nothing new.
    capture.write(REG_SD_STS, 1, u64::from(SDSTS_BCIS));
    assert_eq!(drain(&mut capture.ctrl), 0);
    capture.write(REG_CAPTURE_SD_STS, 1, u64::from(SDSTS_BCIS));
    assert_eq!(drain(&mut capture.ctrl), 0);
    assert!(!capture.ctrl.interrupt_level());
    // hda poll's msis_sent still counts only playback's and the controller's.
    let poll = &capture.ctrl.diagnostic_records()[0];
    assert!(poll.ends_with(" msis_sent=1"), "{poll}");
}
