//! Pasteboard observation and self-write suppression without native API access.

use super::{host_image, HostClip, HostImage, PasteboardBackend};
use sha2::{Digest, Sha256};

/// Ticks to keep reading a pasteboard that a new changeCount showed empty: a
/// Mac copy is clearContents (which moves changeCount) and then the write
/// (which does not), so a tick between the two sees nothing, and the copy
/// never shows up as a change. About 3 s at the 100 ms poll.
pub(super) const EMPTY_REREADS: u8 = 30;

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
    /// Digest of the image last reported or written (images on only).
    last_image: Option<[u8; 32]>,
    /// The last report held an image over the limits (it has no digest).
    last_oversized: bool,
    /// Digest of the `public.html` last reported or written (rich text on only).
    last_html: Option<[u8; 32]>,
    pub(super) observed_suppressed_change: bool,
    /// Also read the pasteboard's image: the guest agent advertised IMAGE_PNG.
    pub(super) images: bool,
    /// Also read the pasteboard's HTML: the guest agent advertised CF_HTML.
    pub(super) html: bool,
    /// Reads left for a copy that may still be between clear and write.
    pub(super) empty_rereads: u8,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // -1 sentinel: the first on_tick always reads the pasteboard, so
            // startup content is emitted once (host wins at startup).
            last_count: -1,
            last_seen: None,
            last_image: None,
            last_oversized: false,
            last_html: None,
            observed_suppressed_change: false,
            images: false,
            html: false,
            empty_rereads: 0,
        }
    }

    /// One poll step: Some(text) iff the pasteboard changed externally to a
    /// non-empty text we haven't already emitted or set ourselves.
    #[cfg(test)]
    pub fn on_tick<B: PasteboardBackend>(&mut self, backend: &mut B) -> Option<String> {
        self.on_tick_clip(backend).and_then(|clip| clip.text)
    }

    /// One poll step: the change iff the pasteboard changed externally to a
    /// non-empty text or (images on) an image we haven't already emitted or set.
    pub fn on_tick_clip<B: PasteboardBackend>(&mut self, backend: &mut B) -> Option<HostClip> {
        self.observe(backend, false)
    }

    pub(super) fn report_current<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
    ) -> Option<HostClip> {
        self.observe(backend, true)
    }

    #[cfg(test)]
    pub(super) fn report_images_on<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
    ) -> Option<HostClip> {
        self.report_formats_on(backend, true, self.html)
    }

    /// Images or rich text were just switched on: report the current content
    /// once when it holds what is now read, so a Mac image or rich copy reaches
    /// the guest the way startup text does. A text change this read happens to
    /// see is reported as on a tick.
    pub(super) fn report_formats_on<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
        images: bool,
        html: bool,
    ) -> Option<HostClip> {
        if !images {
            self.images_off();
        }
        if !html {
            self.rich_text_off();
        }
        (self.images, self.html) = (images, html);
        let seen = self.last_seen.clone();
        let clip = self.observe(backend, true)?;
        (clip.image.is_some() || clip.html.is_some() || clip.text != seen).then_some(clip)
    }

    pub(super) fn images_off(&mut self) {
        self.images = false;
        self.last_image = None;
        self.last_oversized = false;
    }

    pub(super) fn rich_text_off(&mut self) {
        self.html = false;
        self.last_html = None;
    }

    fn observe<B: PasteboardBackend>(&mut self, backend: &mut B, force: bool) -> Option<HostClip> {
        self.observed_suppressed_change = false;
        let count = backend.change_count();
        let new_count = count != self.last_count;
        if !force && !new_count {
            if self.empty_rereads == 0 {
                return None;
            }
            self.empty_rereads -= 1;
        }
        let changed = self.last_count != -1 && new_count;
        self.last_count = count;
        let text = backend.get_string().filter(|text| !text.is_empty());
        let image = if self.images {
            backend.get_image().and_then(host_image)
        } else {
            None
        };
        // Rich text rides with its text, never with an image (which keeps its own path).
        let html = if self.html && text.is_some() && image.is_none() {
            backend.get_html().filter(|html| !html.trim().is_empty())
        } else {
            None
        };
        if text.is_none() && image.is_none() {
            if new_count {
                self.empty_rereads = EMPTY_REREADS;
            }
            // A changed owner is newer even when no transferable content exists.
            // Preserve initial empty-startup behavior and never send a clear.
            self.last_seen = None;
            self.last_image = None;
            self.last_oversized = false;
            self.last_html = None;
            self.observed_suppressed_change = changed && !force;
            return None;
        }
        self.empty_rereads = 0;
        let digest = match &image {
            Some(HostImage::Png(png)) => Some(png.digest),
            _ => None,
        };
        // A too-large image is reported once per copy so its skip is logged,
        // and so is its going away (the copy that replaced it).
        let oversized = matches!(image, Some(HostImage::TooLarge { .. }));
        let new_image = digest != self.last_image || oversized || self.last_oversized;
        let html_digest = html.as_deref().map(html_digest);
        if !force && self.last_seen == text && !new_image && self.last_html == html_digest {
            self.observed_suppressed_change = true;
            return None;
        }
        self.last_seen = text.clone();
        self.last_image = digest;
        self.last_oversized = oversized;
        self.last_html = html_digest;
        Some(HostClip { text, image, html })
    }
}

/// What the poller remembers of a Mac HTML document.
pub(super) fn html_digest(html: &str) -> [u8; 32] {
    Sha256::digest(html.as_bytes()).into()
}

#[path = "host_pasteboard_write.rs"]
mod write;
