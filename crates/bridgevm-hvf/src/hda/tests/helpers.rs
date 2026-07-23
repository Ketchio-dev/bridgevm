//! Split test module.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use std::{path::Path, time::Duration};

pub(super) const RAM_BASE: u64 = 0x1000_0000;

pub(super) fn write(
    ctrl: &mut HdaController,
    mem: &mut FlatGuestRam,
    off: u64,
    size: u8,
    value: u64,
) {
    ctrl.mmio_write(off, size, value, mem);
}

pub(super) fn verb(codec: u8, nid: u8, verb: u16, payload: u8) -> u32 {
    (u32::from(codec) << 28) | (u32::from(nid) << 20) | (u32::from(verb) << 8) | u32::from(payload)
}

pub(super) fn verb16(codec: u8, nid: u8, verb: u8, payload: u16) -> u32 {
    (u32::from(codec) << 28) | (u32::from(nid) << 20) | (u32::from(verb) << 16) | u32::from(payload)
}

pub(super) fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "bridgevm-hda-{label}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RecordedPcm {
    pub(super) samples: Vec<u8>,
    pub(super) rate: u32,
    pub(super) channels: u8,
    pub(super) bits: u8,
}

pub(super) struct RecordingPcmSink {
    pub(super) writes: Arc<Mutex<Vec<RecordedPcm>>>,
}

impl HdaPcmSink for RecordingPcmSink {
    fn write_pcm(&mut self, samples: &[u8], rate: u32, channels: u8, bits: u8) {
        self.writes.lock().unwrap().push(RecordedPcm {
            samples: samples.to_vec(),
            rate,
            channels,
            bits,
        });
    }
}

#[test]
fn stream_bdl_dma_dispatches_pcm_and_running_format_to_trait_sink() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let bdl = RAM_BASE + 0x1000;
    let pcm = RAM_BASE + 0x2000;
    let expected: Vec<u8> = (0..192).map(|value| value as u8).collect();
    assert!(mem.write_bytes(pcm, &expected));
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
    descriptor[8..12].copy_from_slice(&(expected.len() as u32).to_le_bytes());
    assert!(mem.write_bytes(bdl, &descriptor));

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, expected.len() as u64);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(
        *writes.lock().unwrap(),
        vec![RecordedPcm {
            samples: expected,
            rate: 48_000,
            channels: 2,
            bits: 16,
        }]
    );
}

#[test]
fn file_pcm_sink_preserves_bytes_exactly() {
    let output = temp_path("file-sink.raw");
    fs::remove_file(&output).ok();
    let mut sink = FilePcmSink::create(&output).unwrap();

    sink.write_pcm(&[0x00, 0x7f, 0x80], 48_000, 2, 16);
    sink.write_pcm(&[0xff, 0x12, 0x34], 44_100, 1, 16);
    drop(sink);

    assert_eq!(
        fs::read(&output).unwrap(),
        [0x00, 0x7f, 0x80, 0xff, 0x12, 0x34]
    );
    fs::remove_file(output).ok();
}
