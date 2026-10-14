//! Nonblocking channels and the single pasteboard worker. The worker steps
//! accept a backend so service-ordering tests never touch NSPasteboard.

use super::{default_backend, Poller};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[path = "host_pasteboard_worker.rs"]
mod worker;
pub(crate) use worker::PasteboardWorker;

pub struct HostPasteboard {
    events: Receiver<ClipboardText>,
    cmds: Sender<PasteboardWrite>,
    observed: Cell<u64>,
    recopy: Arc<AtomicU64>,
}

struct ClipboardText {
    revision: u64,
    text: String,
}

struct PasteboardWrite {
    revision: u64,
    recopy: u64,
    text: String,
}

impl HostPasteboard {
    pub fn spawn(poll_ms: u64) -> Self {
        let (pasteboard, worker) = Self::pair();
        let poll = Duration::from_millis(poll_ms.max(50));
        let _ = thread::Builder::new()
            .name("bv-pasteboard".into())
            .spawn(move || worker::run(worker, default_backend(), poll));
        pasteboard
    }

    fn pair() -> (Self, PasteboardWorker) {
        let (event_tx, events) = channel();
        let (cmds, cmd_rx) = channel();
        let recopy = Arc::new(AtomicU64::new(0));
        (
            Self {
                events,
                cmds,
                observed: Cell::new(0),
                recopy: Arc::clone(&recopy),
            },
            PasteboardWorker {
                events: event_tx,
                cmds: cmd_rx,
                poller: Poller::new(),
                revision: Some(0),
                initialized: false,
                recopy,
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
        let _ = self.cmds.send(PasteboardWrite {
            revision: self.observed.get(),
            // Orders worker-observed duplicate ownership, not native event time.
            recopy: self.recopy.load(Ordering::Acquire),
            text,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_pair() -> (Self, PasteboardWorker) {
        Self::pair()
    }
}
