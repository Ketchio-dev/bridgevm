//! Nonblocking channels and the single pasteboard worker. The worker steps
//! accept a backend so service-ordering tests never touch NSPasteboard.

use super::{default_backend, PasteboardBackend, Poller};
use std::cell::Cell;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

#[cfg(test)]
#[path = "host_pasteboard_bridge_tests.rs"]
mod tests;

pub struct HostPasteboard {
    events: Receiver<ClipboardText>,
    cmds: Sender<ClipboardText>,
    observed: Cell<u64>,
}

pub(crate) struct PasteboardWorker {
    events: Sender<ClipboardText>,
    cmds: Receiver<ClipboardText>,
    poller: Poller,
    revision: Option<u64>,
    initialized: bool,
}

struct ClipboardText {
    revision: u64,
    text: String,
}

impl HostPasteboard {
    pub fn spawn(poll_ms: u64) -> Self {
        let (pasteboard, worker) = Self::pair();
        let poll = Duration::from_millis(poll_ms.max(50));
        let _ = thread::Builder::new()
            .name("bv-pasteboard".into())
            .spawn(move || run(worker, default_backend(), poll));
        pasteboard
    }

    fn pair() -> (Self, PasteboardWorker) {
        let (event_tx, events) = channel();
        let (cmds, cmd_rx) = channel();
        (
            Self {
                events,
                cmds,
                observed: Cell::new(0),
            },
            PasteboardWorker {
                events: event_tx,
                cmds: cmd_rx,
                poller: Poller::new(),
                revision: Some(0),
                initialized: false,
            },
        )
    }

    /// Next external host-pasteboard change, without blocking.
    pub fn try_changed(&self) -> Option<String> {
        let event = self.events.try_recv().ok()?;
        self.observed.set(event.revision);
        Some(event.text)
    }

    /// Queue a guest-to-host write without blocking.
    pub fn set(&self, text: String) {
        let _ = self.cmds.send(ClipboardText {
            revision: self.observed.get(),
            text,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_pair() -> (Self, PasteboardWorker) {
        Self::pair()
    }
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
        for write in pending {
            // A newer Mac report may still be waiting in the event channel.
            // Only the service's last received revision can authorize a write.
            if Some(write.revision) == self.revision {
                self.poller.apply_set(backend, &write.text);
            }
        }
        true
    }

    pub(crate) fn poll<B: PasteboardBackend>(&mut self, backend: &mut B) -> bool {
        self.initialized = true;
        match self.poller.on_tick(backend) {
            Some(text) => self.report(text),
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

fn run<B: PasteboardBackend>(mut worker: PasteboardWorker, mut backend: B, poll: Duration) {
    while worker.apply_pending(&mut backend) && worker.poll(&mut backend) {
        thread::sleep(poll);
    }
}
