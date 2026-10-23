//! The lock-free "stream 0 is running" flag a host service thread waits on.
//!
//! Guest PCM advances only when something polls the controller: a vCPU exit's
//! drain, under the platform lock, and, while this flag is set, the probe's
//! host service tick (examples/hvf_gic_boot_probe/hda_service_tick.rs). The
//! flag mirrors SD0CTL.RUN. The controller sets it on the RUN rise
//! (stream_control.rs) and clears it on every RUN clear: a guest stop and a
//! stream reset (SRST, both stream_control.rs), a DMA halt on a descriptor or
//! memory fault (`halt_stream_dma`, from playback_dma.rs), and every controller
//! replacement (`replace_keeping_diagnostics`), which a controller reset
//! (stream_lifecycle.rs) and a platform reset make. Only the controller writes
//! it, always under the platform lock; a reader needs no lock. A rise unparks
//! the registered waiter, so a parked thread wakes without polling the flag.
//! A replacement keeps the same handle, so a host takes it once per process.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, OnceLock};
use std::thread::{self, Thread};

/// Whether the playback stream runs, readable without the platform lock.
#[derive(Debug, Default)]
pub struct HdaStreamActivity {
    running: AtomicBool,
    waiter: OnceLock<Thread>,
}

impl HdaStreamActivity {
    /// True while SD0CTL.RUN is set.
    pub fn running(&self) -> bool {
        self.running.load(SeqCst)
    }

    /// Make the current thread the one a RUN rise unparks. False when another
    /// thread already is: a controller has one waiter for the process.
    pub fn register_current_thread(&self) -> bool {
        self.waiter.set(thread::current()).is_ok()
    }

    pub(crate) fn start(&self) {
        if !self.running.swap(true, SeqCst) {
            if let Some(waiter) = self.waiter.get() {
                waiter.unpark();
            }
        }
    }

    pub(crate) fn stop(&self) {
        self.running.store(false, SeqCst);
    }
}

impl HdaController {
    /// The stream-activity flag; every controller replacement keeps it.
    pub fn stream_activity(&self) -> Arc<HdaStreamActivity> {
        Arc::clone(&self.stream_activity)
    }

    /// A descriptor or memory fault halts DMA: DESE set, RUN and flag clear.
    pub(crate) fn halt_stream_dma(&mut self) {
        self.stream.sts |= SDSTS_DESE;
        self.stream.ctl &= !SDCTL_RUN;
        self.stream_activity.stop();
    }

    /// Reinitialize the controller as `fresh`, keeping run-lifetime diagnostics
    /// and the stream-activity handle, which it clears: `fresh` is stopped.
    /// Moved here from diagnostics.rs when it began keeping the handle.
    pub(crate) fn replace_keeping_diagnostics(&mut self, fresh: Self) {
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let activity = Arc::clone(&self.stream_activity);
        *self = fresh;
        self.diagnostics = diagnostics;
        activity.stop();
        self.stream_activity = activity;
    }
}
