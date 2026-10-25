//! The stream-activity flag mirrors SD0CTL.RUN through every transition, keeps
//! its handle across controller resets, and wakes its parked waiter on a rise.

use super::super::*;
use super::helpers::{write, RecordingPcmSink, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::{path::Path, time::Duration};

const BDL: u64 = RAM_BASE + 0x1000;
const PCM: u64 = RAM_BASE + 0x2000;

struct Stream {
    ctrl: HdaController,
    mem: FlatGuestRam,
}

impl Stream {
    /// Two 192-byte entries at PCM, programmed but not running.
    fn programmed() -> Self {
        let mut stream = Self {
            ctrl: HdaController::with_pcm_output_path::<&Path>(None),
            mem: FlatGuestRam::new(RAM_BASE, 0x10000),
        };
        for index in 0..2u64 {
            stream.descriptor(index, PCM + index * 192, 192);
        }
        stream.program();
        stream
    }

    /// What a guest driver writes before RUN, after any reset.
    fn program(&mut self) {
        for (offset, size, value) in [
            (REG_GCTL, 4, 1),
            (REG_SD_BDPL, 4, BDL),
            (REG_SD_CBL, 4, 384),
            (REG_SD_LVI, 2, 1),
            (REG_SD_FMT, 2, 0x0011),
        ] {
            write(&mut self.ctrl, &mut self.mem, offset, size, value);
        }
    }

    fn descriptor(&mut self, index: u64, address: u64, length: u32) {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&address.to_le_bytes());
        descriptor[8..12].copy_from_slice(&length.to_le_bytes());
        assert!(self.mem.write_bytes(BDL + index * 16, &descriptor));
    }

    fn ctl(&mut self, value: u32) {
        let value = u64::from(value);
        write(&mut self.ctrl, &mut self.mem, REG_SD_CTL, 3, value);
    }

    fn running(&mut self) -> Arc<HdaStreamActivity> {
        let activity = self.ctrl.stream_activity();
        self.ctl(SDCTL_RUN);
        self.assert_mirrors_run(true);
        activity
    }

    /// The flag and SD0CTL.RUN agree, and equal `expected`.
    fn assert_mirrors_run(&self, expected: bool) {
        let run = self.ctrl.mmio_read(REG_SD_CTL, 1) & u64::from(SDCTL_RUN) != 0;
        assert_eq!(run, expected, "SD0CTL.RUN");
        assert_eq!(self.ctrl.stream_activity().running(), expected, "flag");
    }
}

#[test]
fn the_flag_follows_run_through_start_stop_resume_and_stream_reset() {
    let mut stream = Stream::programmed();
    stream.assert_mirrors_run(false);
    let activity = stream.running();
    stream.ctl(SDCTL_RUN | SDCTL_IOCE);
    stream.assert_mirrors_run(true);
    stream.ctl(0);
    stream.assert_mirrors_run(false);
    assert!(!activity.running());
    stream.ctl(SDCTL_RUN);
    stream.assert_mirrors_run(true);
    stream.ctl(SDCTL_SRST | SDCTL_RUN);
    stream.assert_mirrors_run(false);
    stream.ctl(0);
    stream.ctl(SDCTL_RUN);
    stream.assert_mirrors_run(true);
    assert!(Arc::ptr_eq(&activity, &stream.ctrl.stream_activity()));
}

#[test]
fn every_dma_halt_clears_the_flag() {
    // An unreadable descriptor list, a zero-length entry, unreadable PCM.
    let faults: [fn(&mut Stream); 3] = [
        |stream| {
            write(
                &mut stream.ctrl,
                &mut stream.mem,
                REG_SD_BDPL,
                4,
                0x7f00_0000,
            )
        },
        |stream| stream.descriptor(0, PCM, 0),
        |stream| stream.descriptor(0, RAM_BASE + 0x20_0000, 192),
    ];
    for fault in faults {
        let mut stream = Stream::programmed();
        fault(&mut stream);
        let activity = stream.running();
        stream
            .ctrl
            .poll_for_duration(&mut stream.mem, Duration::from_millis(1));
        stream.assert_mirrors_run(false);
        assert_ne!(
            stream.ctrl.mmio_read(REG_SD_STS, 1) & u64::from(SDSTS_DESE),
            0
        );
        assert!(!activity.running());
    }
}

#[test]
fn controller_and_platform_resets_clear_the_flag_and_keep_its_handle() {
    // GCTL.CRST cleared by the guest: a controller reset.
    let mut stream = Stream::programmed();
    let activity = stream.running();
    write(&mut stream.ctrl, &mut stream.mem, REG_GCTL, 4, 0);
    stream.assert_mirrors_run(false);
    assert!(Arc::ptr_eq(&activity, &stream.ctrl.stream_activity()));
    // A platform reset with a host sink installed (controller_reset) and
    // without one (a fresh controller from the environment).
    for overridden in [true, false] {
        let mut stream = Stream::programmed();
        if overridden {
            let writes = Arc::new(Mutex::new(Vec::new()));
            stream
                .ctrl
                .set_pcm_sink(Some(Box::new(RecordingPcmSink { writes })));
        }
        let activity = stream.running();
        stream.ctrl.reset_runtime_state();
        stream.assert_mirrors_run(false);
        assert!(!activity.running());
        // The next boot generation's RUN raises the same handle.
        stream.program();
        stream.ctl(SDCTL_RUN);
        stream.assert_mirrors_run(true);
        assert!(Arc::ptr_eq(&activity, &stream.ctrl.stream_activity()));
        assert!(activity.running());
    }
}

#[test]
fn a_run_rise_unparks_the_registered_waiter_even_before_it_parks() {
    let mut stream = Stream::programmed();
    let activity = stream.ctrl.stream_activity();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (awake_tx, awake_rx) = mpsc::channel();
    let proceed = Arc::new(AtomicBool::new(false));
    let (waiter_activity, waiter_proceed) = (Arc::clone(&activity), Arc::clone(&proceed));
    let waiter = std::thread::spawn(move || {
        assert!(waiter_activity.register_current_thread());
        ready_tx.send(()).unwrap();
        while !waiter_proceed.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        while !waiter_activity.running() {
            std::thread::park();
        }
        awake_tx.send(()).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        !activity.register_current_thread(),
        "one waiter per controller"
    );
    stream.ctl(SDCTL_RUN);
    proceed.store(true, Ordering::Release);
    awake_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    waiter.join().unwrap();
}
