//! Nonblocking channels and the single pasteboard worker. The worker steps
//! accept a backend so service-ordering tests never touch NSPasteboard.

use super::{default_backend, HostImage, PngImage, Poller};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    images: Arc<AtomicBool>,
    html: Arc<AtomicBool>,
}

/// One Mac pasteboard change: its text and, while images are on, its image;
/// while rich text is on, a text copy's `public.html`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostClip {
    pub text: Option<String>,
    pub image: Option<HostImage>,
    pub html: Option<String>,
}

struct ClipboardText {
    revision: u64,
    clip: HostClip,
}

struct PasteboardWrite {
    revision: u64,
    recopy: u64,
    /// Empty when an image is written without text.
    text: String,
    image: Option<PngImage>,
    /// The text's `public.html` (never with an image).
    html: Option<String>,
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
        let images = Arc::new(AtomicBool::new(false));
        let html = Arc::new(AtomicBool::new(false));
        (
            Self {
                events,
                cmds,
                observed: Cell::new(0),
                recopy: Arc::clone(&recopy),
                images: Arc::clone(&images),
                html: Arc::clone(&html),
            },
            PasteboardWorker {
                events: event_tx,
                cmds: cmd_rx,
                poller: Poller::new(),
                revision: Some(0),
                initialized: false,
                recopy,
                images,
                html,
            },
        )
    }

    /// Next external host-pasteboard change, without blocking.
    pub fn try_changed_clip(&self) -> Option<HostClip> {
        let event = self.events.try_recv().ok()?;
        self.observed.set(event.revision);
        Some(event.clip)
    }

    /// The text of the next external change (text-only tests).
    #[cfg(test)]
    pub fn try_changed(&self) -> Option<String> {
        self.try_changed_clip().and_then(|clip| clip.text)
    }

    /// Queue a guest-to-host write without blocking.
    pub fn set(&self, text: String) {
        self.write(text, None, None);
    }

    /// Queue a guest-to-host image write, with the guest's text when it has one.
    pub fn set_image(&self, png: PngImage, text: Option<String>) {
        self.write(text.unwrap_or_default(), Some(png), None);
    }

    /// Queue a guest-to-host rich text write: the text and its HTML at once.
    pub fn set_text_html(&self, text: String, html: String) {
        self.write(text, None, Some(html));
    }

    /// Read the Mac pasteboard's image too: the agent can take images.
    pub fn set_images(&self, on: bool) {
        self.images.store(on, Ordering::Release);
    }

    /// Read the Mac pasteboard's HTML too: the agent can take rich text.
    pub fn set_rich_text(&self, on: bool) {
        self.html.store(on, Ordering::Release);
    }

    fn write(&self, text: String, image: Option<PngImage>, html: Option<String>) {
        let _ = self.cmds.send(PasteboardWrite {
            revision: self.observed.get(),
            // Orders suppressed host ownership changes, not native event time.
            recopy: self.recopy.load(Ordering::Acquire),
            text,
            image,
            html,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_pair() -> (Self, PasteboardWorker) {
        Self::pair()
    }
}
