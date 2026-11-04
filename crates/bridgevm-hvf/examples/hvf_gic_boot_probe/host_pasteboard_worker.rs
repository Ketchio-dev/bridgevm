//! One worker owns observation order; epochs do not make native writes atomic.

use super::super::{PasteboardBackend, Poller};
#[cfg(test)]
use super::HostPasteboard;
use super::{ClipboardText, HostClip, PasteboardWrite};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pub(super) images: Arc<AtomicBool>,
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
                    match &write.image {
                        Some(png) => {
                            let text = Some(write.text.as_str()).filter(|text| !text.is_empty());
                            self.poller.apply_set_image(backend, png, text);
                        }
                        None => self.poller.apply_set(backend, &write.text),
                    }
                    fresh = true;
                } else {
                    refused = true;
                }
            }
        }
        // A later guest write in this FIFO batch owns the clipboard. Otherwise
        // Re-report refused host text when representable; never invent a clear.
        if refused && !fresh {
            if let Some(clip) = self.poller.report_current(backend) {
                return self.report_clip(clip);
            }
        }
        true
    }

    pub(crate) fn poll<B: PasteboardBackend>(&mut self, backend: &mut B) -> bool {
        self.initialized = true;
        let images = self.images.load(Ordering::Acquire);
        let change = if images && !self.poller.images {
            self.poller.report_images_on(backend)
        } else {
            if !images && self.poller.images {
                self.poller.images_off();
            }
            self.poller.on_tick_clip(backend)
        };
        match change {
            Some(clip) => self.report_clip(clip),
            None if self.poller.observed_suppressed_change => {
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

    #[cfg(test)]
    pub(crate) fn report(&mut self, text: String) -> bool {
        self.report_clip(HostClip {
            text: Some(text),
            image: None,
        })
    }

    pub(crate) fn report_clip(&mut self, clip: HostClip) -> bool {
        // Exhaustion terminates the worker instead of wrapping to an old
        // accepted revision. Guest writes never advance this host revision.
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
        let Some(revision) = self.revision else {
            return false;
        };
        self.events.send(ClipboardText { revision, clip }).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn take_sets(&self) -> Vec<String> {
        self.cmds.try_iter().map(|write| write.text).collect()
    }

    /// Queued guest writes as (text, image) pairs; text is empty without one.
    #[cfg(test)]
    pub(crate) fn take_writes(&self) -> Vec<(String, Option<super::PngImage>)> {
        self.cmds
            .try_iter()
            .map(|write| (write.text, write.image))
            .collect()
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
