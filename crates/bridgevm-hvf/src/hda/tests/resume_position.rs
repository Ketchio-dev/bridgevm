//! Intel HDA 1.0a section 4.5.5 requires RUN resume at the prior DMA position.
//! These public-MMIO/DMA cases inspect both LPIB and the bytes sent to the sink.

use super::super::*;
use super::helpers::{write, RecordedPcm, RecordingPcmSink, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

fn assert_resume_at(first_poll: Duration, position: usize, resumed_poll: Duration) {
    let writes: Arc<Mutex<Vec<RecordedPcm>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let (bdl, pcm) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
    let expected: Vec<u8> = (0..384).map(|index| (index % 251) as u8).collect();
    assert!(mem.write_bytes(pcm, &expected));
    for index in 0..2u64 {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&(pcm + index * 192).to_le_bytes());
        descriptor[8..12].copy_from_slice(&192u32.to_le_bytes());
        assert!(mem.write_bytes(bdl + index * 16, &descriptor));
    }
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 384);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 1);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    let ctl = |ctrl: &mut HdaController, mem: &mut FlatGuestRam, value: u32| {
        write(ctrl, mem, REG_SD_CTL, 3, u64::from((1 << 20) | value));
    };
    ctl(&mut ctrl, &mut mem, SDCTL_RUN);
    ctrl.poll_for_duration(&mut mem, first_poll);
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), position as u64);
    assert_eq!(recorded_bytes(&writes), expected[..position]);

    ctl(&mut ctrl, &mut mem, 0);
    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN), 0);
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), position as u64);
    assert_eq!(recorded_bytes(&writes), expected[..position]);
    ctl(&mut ctrl, &mut mem, SDCTL_RUN);
    ctrl.poll_for_duration(&mut mem, resumed_poll);

    assert_eq!(
        (ctrl.mmio_read(REG_SD_LPIB, 4), recorded_bytes(&writes)),
        (288, expected[..288].to_vec()),
        "RUN resume must continue the descriptor offset/index without replaying PCM",
    );
    for chunk in writes.lock().unwrap().iter() {
        assert_eq!((chunk.rate, chunk.channels, chunk.bits), (48_000, 2, 16));
    }
}

fn recorded_bytes(writes: &Arc<Mutex<Vec<RecordedPcm>>>) -> Vec<u8> {
    writes
        .lock()
        .unwrap()
        .iter()
        .flat_map(|chunk| chunk.samples.iter().copied())
        .collect()
}

#[test]
fn run_resume_preserves_partial_descriptor_position_and_pcm() {
    assert_resume_at(Duration::from_micros(500), 96, Duration::from_millis(1));
}

#[test]
fn run_resume_preserves_next_descriptor_position_and_pcm() {
    assert_resume_at(Duration::from_millis(1), 192, Duration::from_micros(500));
}
