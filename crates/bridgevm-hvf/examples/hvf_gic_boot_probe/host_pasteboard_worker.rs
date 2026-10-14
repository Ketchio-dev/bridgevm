//! One worker owns observation order; epochs do not make native writes atomic.

use super::super::{PasteboardBackend, Poller};
#[cfg(test)]
use super::HostPasteboard;
use super::{ClipboardText, PasteboardWrite};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
#[cfg(test)]
#[path = "host_pasteboard_recopy_tests.rs"]
mod recopy_tests;
#[cfg(test)]
#[path = "host_pasteboard_bridge_tests.rs"]
mod tests;

pub(crate) struct PasteboardWorker {
    pub(super) events: Sender<ClipboardText>,
    pub(super) cmds: Receiver<PasteboardWrite>,
    pub(super) poller: Poller,
    pub(super) revision: Option<u64>,
    pub(super) initialized: bool,
    pub(super) recopy: Arc<AtomicU64>,
}

impl PasteboardWorker {
    pub(crate) fn apply_pending<B: PasteboardBackend>(&mut self, backend: &mut B) -> bool {
        if self.revision.is_none() || (!self.initialized && !self.poll(backend)) {
            return false;
        }
        // Discard the batch if its service owner already disconnected. Each
        // fresh bridge has distinct channels; old writes cannot migrate to it.
        let mut pending = Vec::new();
        loop {
            match self.cmds.try_recv() {
                Ok(text) => pending.push(text),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
        let (mut refused, mut fresh) = (false, false);
        for write in pending {
            // A newer Mac report may still be waiting in the event channel.
            // Only the service's last received revision can authorize a write.
            if Some(write.revision) == self.revision {
                if write.recopy == self.recopy.load(Ordering::Acquire) {
                    self.poller.apply_set(backend, &write.text);
                    fresh = true;
                } else {
                    refused = true;
                }
            }
        }
        // A later guest write in this FIFO batch owns the clipboard. Otherwise
        // restore the refused recopy to the service with a new report revision.
        if refused && !fresh {
            if let Some(text) = self.poller.report_current(backend) {
                return self.report(text);
            }
        }
        true
    }

    pub(crate) fn poll<B: PasteboardBackend>(&mut self, backend: &mut B) -> bool {
        self.initialized = true;
        match self.poller.on_tick(backend) {
            Some(text) => self.report(text),
            None if self.poller.observed_recopy => {
                let Some(next) = self.recopy.load(Ordering::Relaxed).checked_add(1) else {
                    self.revision = None;
                    return false;
                };
                self.recopy.store(next, Ordering::Release);
                true
            }
            None => true,
        }
    }

    pub(crate) fn report(&mut self, text: String) -> bool {
        // Exhaustion terminates the worker instead of wrapping to an old
        // accepted revision. Guest writes never advance this host revision.
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
        let Some(revision) = self.revision else {
            return false;
        };
        self.events.send(ClipboardText { revision, text }).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn take_sets(&self) -> Vec<String> {
        self.cmds.try_iter().map(|write| write.text).collect()
    }
}

pub(super) fn run<B: PasteboardBackend>(
    mut worker: PasteboardWorker,
    mut backend: B,
    poll: Duration,
) {
    while worker.apply_pending(&mut backend) && worker.poll(&mut backend) {
        thread::sleep(poll);
    }
}
