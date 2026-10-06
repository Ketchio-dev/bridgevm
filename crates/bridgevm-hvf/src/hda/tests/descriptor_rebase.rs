//! Reprogramming a stopped stream must not wrap its saved descriptor address.

use super::super::*;
use super::helpers::{write, RecordedPcm, RecordingPcmSink, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

struct ObservedMemory {
    ram: FlatGuestRam,
    reads: std::cell::RefCell<Vec<u64>>,
}

impl GuestMemoryMut for ObservedMemory {
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.reads.borrow_mut().push(gpa);
        self.ram.read_bytes(gpa, len)
    }

    fn read_into(&self, gpa: u64, out: &mut [u8]) -> bool {
        self.reads.borrow_mut().push(gpa);
        self.ram.read_into(gpa, out)
    }

    fn write_bytes(&mut self, gpa: u64, bytes: &[u8]) -> bool {
        self.ram.write_bytes(gpa, bytes)
    }
}

fn assert_rebased_descriptor_failure(base: u64, expected_read: Option<u64>) {
    let writes: Arc<Mutex<Vec<RecordedPcm>>> = Arc::default();
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    ctrl.set_pcm_sink(Some(Box::new(RecordingPcmSink {
        writes: Arc::clone(&writes),
    })));
    let mut ram = FlatGuestRam::new(RAM_BASE, 0x10000);
    let (bdl, pcm, position) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000, RAM_BASE + 0x3000);
    assert!(ram.write_bytes(pcm, &[0x5a; 192]));
    for index in 0..16u64 {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
        descriptor[8..12].copy_from_slice(&192u32.to_le_bytes());
        assert!(ram.write_bytes(bdl + index * 16, &descriptor));
    }
    write(&mut ctrl, &mut ram, REG_GCTL, 4, 1);
    write(&mut ctrl, &mut ram, REG_DPLBASE, 4, position | 1);
    write(&mut ctrl, &mut ram, REG_SD_BDPL, 4, bdl);
    write(&mut ctrl, &mut ram, REG_SD_CBL, 4, 16 * 192);
    write(&mut ctrl, &mut ram, REG_SD_LVI, 2, 15);
    write(&mut ctrl, &mut ram, REG_SD_FMT, 2, 0x0011);
    write(&mut ctrl, &mut ram, REG_SD_CTL, 1, u64::from(SDCTL_RUN));
    ctrl.poll_for_duration(&mut ram, Duration::from_millis(8));
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), 8 * 192);
    assert_eq!((ctrl.stream.bdl_index, ctrl.stream.bdl_offset), (8, 0));
    let output_before = writes.lock().unwrap().clone();
    assert_eq!(output_before.len(), 8);
    let position_before = ram.read_bytes(position, 4).unwrap();

    write(&mut ctrl, &mut ram, REG_SD_CTL, 1, 0);
    write(&mut ctrl, &mut ram, REG_SD_BDPL, 4, base as u32 as u64);
    write(&mut ctrl, &mut ram, REG_SD_BDPU, 4, base >> 32);
    write(
        &mut ctrl,
        &mut ram,
        REG_SD_CTL,
        1,
        u64::from(SDCTL_RUN | SDCTL_DEIE),
    );
    write(
        &mut ctrl,
        &mut ram,
        REG_INTCTL,
        4,
        u64::from(INTCTL_GIE | INTCTL_STREAM0),
    );
    let mut mem = ObservedMemory {
        ram,
        reads: Default::default(),
    };
    ctrl.poll_for_duration(&mut mem, Duration::from_millis(1));

    assert_eq!(
        *mem.reads.borrow(),
        expected_read.into_iter().collect::<Vec<_>>()
    );
    assert_eq!(*writes.lock().unwrap(), output_before);
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), 8 * 192);
    assert_eq!(mem.ram.read_bytes(position, 4).unwrap(), position_before);
    assert_eq!((ctrl.stream.bdl_index, ctrl.stream.bdl_offset), (8, 0));
    assert_eq!(ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN), 0);
    assert_eq!(ctrl.mmio_read(REG_SD_STS, 1), u64::from(SDSTS_DESE));
    assert!(ctrl.interrupt_level());
}

#[test]
fn stopped_stream_rebase_overflow_fails_before_guest_read() {
    assert_rebased_descriptor_failure(!0x7fu64, None);
}

#[test]
fn stopped_stream_rebase_unmapped_address_uses_normal_dma_failure() {
    let base = RAM_BASE + 0x20000;
    assert_rebased_descriptor_failure(base, Some(base + 8 * 16));
}

#[test]
fn stopped_stream_rebase_last_aligned_address_does_not_overflow() {
    let address = !0x7fu64;
    assert_rebased_descriptor_failure(address - 8 * 16, Some(address));
}
