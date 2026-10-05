//! A live guest descriptor rewrite must fail DMA rather than overflow its address.

use super::super::*;
use super::helpers::{write, RecordedPcm, RecordingPcmSink, RAM_BASE};
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::FlatGuestRam;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn rewritten_pcm_address_overflow_stops_without_advancing_or_completing_dma() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
    let (bdl, pcm, position) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000, RAM_BASE + 0x3000);
    assert!(mem.write_bytes(pcm, &[0x5a; 384]));
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
    descriptor[8..12].copy_from_slice(&384u32.to_le_bytes());
    descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
    assert!(mem.write_bytes(bdl, &descriptor));

    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut mem, REG_DPLBASE, 4, position | 1);
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 384);
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    write(
        &mut ctrl,
        &mut mem,
        REG_SD_CTL,
        1,
        u64::from(SDCTL_RUN | SDCTL_IOCE),
    );
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    let expected = vec![RecordedPcm {
        samples: vec![0x5a; 192],
        rate: 48_000,
        channels: 2,
        bits: 16,
    }];
    assert_eq!(*writes.lock().unwrap(), expected);
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), 192);
    assert_eq!(ctrl.stream.bdl_offset, 192);
    let position_before = mem.read_bytes(position, 4).unwrap();
    assert_eq!(position_before, 192u32.to_le_bytes());
    assert_eq!(ctrl.mmio_read(REG_SD_STS, 1), 0);

    assert!(mem.write_bytes(bdl, &u64::MAX.to_le_bytes()));
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(*writes.lock().unwrap(), expected);
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), 192);
    assert_eq!(mem.read_bytes(position, 4).unwrap(), position_before);
    assert_eq!((ctrl.stream.bdl_index, ctrl.stream.bdl_offset), (0, 192));
    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN), 0);
    assert_ne!(ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_DESE), 0);
    assert_eq!(ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_BCIS), 0);
}
