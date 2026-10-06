//! RUN transitions preserve completion status; W1C and explicit resets clear it.
//! Intel HDA 1.0a sections 3.3.35–3.3.36 distinguish these operations.

use super::super::*;
use super::helpers::{write, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::{path::Path, time::Duration};

struct Stream {
    ctrl: HdaController,
    mem: FlatGuestRam,
}

impl Stream {
    fn completed() -> Self {
        let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
        let mut mem = FlatGuestRam::new(RAM_BASE, 0x10000);
        let (bdl, pcm) = (RAM_BASE + 0x1000, RAM_BASE + 0x2000);
        for index in 0..2u64 {
            let mut descriptor = [0u8; 16];
            descriptor[..8].copy_from_slice(&(pcm + index * 192).to_le_bytes());
            descriptor[8..12].copy_from_slice(&192u32.to_le_bytes());
            let flags = if index == 0 { BDL_IOC } else { 0 };
            descriptor[12..16].copy_from_slice(&flags.to_le_bytes());
            assert!(mem.write_bytes(bdl + index * 16, &descriptor));
        }
        write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
        write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, bdl);
        write(&mut ctrl, &mut mem, REG_SD_CBL, 4, 384);
        write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 1);
        write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
        write(
            &mut ctrl,
            &mut mem,
            REG_INTCTL,
            4,
            u64::from(INTCTL_GIE | INTCTL_STREAM0),
        );
        let mut stream = Self { ctrl, mem };
        stream.ctl(SDCTL_IOCE | SDCTL_RUN);
        stream
            .ctrl
            .poll_for_duration(&mut stream.mem, Duration::from_millis(1));
        assert_eq!(stream.ctrl.mmio_read(REG_SD_LPIB, 4), 192);
        stream.assert_pending(true);
        stream
    }

    fn ctl(&mut self, value: u32) {
        write(
            &mut self.ctrl,
            &mut self.mem,
            REG_SD_CTL,
            3,
            u64::from((1 << 20) | value),
        );
    }

    fn assert_pending(&self, expected: bool) {
        assert_eq!(
            self.ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_BCIS) != 0,
            expected,
        );
        assert_eq!(self.ctrl.interrupt_level(), expected);
    }
}

#[test]
fn unacknowledged_completion_survives_run_stop_and_restart() {
    let mut stream = Stream::completed();
    stream.ctl(SDCTL_IOCE);
    stream.assert_pending(true);
    stream.ctl(SDCTL_IOCE | SDCTL_RUN);
    stream.assert_pending(true);
}

#[test]
fn acknowledged_completion_does_not_return_on_run_restart() {
    let mut stream = Stream::completed();
    stream.ctl(SDCTL_IOCE);
    write(&mut stream.ctrl, &mut stream.mem, REG_SD_STS, 1, 0);
    stream.assert_pending(true);
    write(
        &mut stream.ctrl,
        &mut stream.mem,
        REG_SD_STS,
        1,
        u64::from(SDSTS_BCIS),
    );
    stream.assert_pending(false);
    stream.ctl(SDCTL_IOCE | SDCTL_RUN);
    stream.assert_pending(false);
}

#[test]
fn explicit_stream_and_controller_resets_clear_completion() {
    let mut stream = Stream::completed();
    stream.ctl(SDCTL_IOCE);
    stream.ctl(SDCTL_SRST);
    assert_ne!(stream.ctrl.mmio_read(REG_SD_CTL, 1) & 1, 0);
    stream.assert_pending(false);
    stream.ctl(0);
    assert_eq!(stream.ctrl.mmio_read(REG_SD_CTL, 1) & 1, 0);
    stream.ctl(SDCTL_IOCE | SDCTL_RUN);
    stream.assert_pending(false);

    let mut stream = Stream::completed();
    stream.ctl(SDCTL_IOCE);
    write(&mut stream.ctrl, &mut stream.mem, REG_GCTL, 4, 0);
    stream.assert_pending(false);
}
