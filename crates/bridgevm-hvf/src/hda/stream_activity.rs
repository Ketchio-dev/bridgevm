//! The lock-free "a stream is running" flag a host service thread waits on.
//!
//! Guest PCM advances only when something polls the controller: a vCPU exit's
//! drain, under the platform lock, and, while this flag is set, the probe's
//! host service tick (examples/hvf_gic_boot_probe/hda_service_tick.rs). The
//! flag is set while the playback or the capture stream's RUN is: capture DMA
//! advances only when polled, as playback does. The controller sets it on a RUN rise
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

/// Whether a stream runs, readable without the platform lock.
#[derive(Debug, Default)]
pub struct HdaStreamActivity {
    running: AtomicBool,
    waiter: OnceLock<Thread>,
}

impl HdaStreamActivity {
    /// True while the playback or the capture stream's RUN is set.
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
        self.sync_stream_activity();
    }

    /// The flag follows both streams' RUN bits.
    pub(crate) fn sync_stream_activity(&self) {
        if (self.stream.ctl | self.capture.ctl) & SDCTL_RUN != 0 {
            self.stream_activity.start();
        } else {
            self.stream_activity.stop();
        }
    }

    /// Reinitialize the controller as `fresh`, keeping run-lifetime diagnostics
    /// and the stream-activity handle, which it clears: `fresh` is stopped.
    /// Moved here from diagnostics.rs when it began keeping the handle.
    /// The host's capture source survives too: the host installed it, as the
    /// activity handle, and a running capture stream is told it stopped.
    pub(crate) fn replace_keeping_diagnostics(&mut self, fresh: Self) {
        self.notify_capture_stopped(self.capture.ctl & SDCTL_RUN != 0);
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let activity = Arc::clone(&self.stream_activity);
        let source = self.pcm_source.take();
        let microphone = self.microphone;
        *self = fresh;
        self.diagnostics = diagnostics;
        activity.stop();
        self.stream_activity = activity;
        self.pcm_source = source;
        self.microphone = microphone;
    }
}
