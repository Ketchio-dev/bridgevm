//! Pasteboard observation and self-write suppression without native API access.

use super::{host_image, HostClip, HostImage, PasteboardBackend};

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
    /// Digest of the image last reported or written (images on only).
    last_image: Option<[u8; 32]>,
    /// The last report held an image over the limits (it has no digest).
    last_oversized: bool,
    pub(super) observed_suppressed_change: bool,
    /// Also read the pasteboard's image: the guest agent advertised IMAGE_PNG.
    pub(super) images: bool,
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
            observed_suppressed_change: false,
            images: false,
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

    /// Images were just switched on: report the current content once when it
    /// holds an image, so a Mac image reaches the guest the way startup text
    /// does. A text change this read happens to see is reported as on a tick.
    pub(super) fn report_images_on<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
    ) -> Option<HostClip> {
        self.images = true;
        let seen = self.last_seen.clone();
        let clip = self.observe(backend, true)?;
        (clip.image.is_some() || clip.text != seen).then_some(clip)
    }

    pub(super) fn images_off(&mut self) {
        self.images = false;
        self.last_image = None;
        self.last_oversized = false;
    }

    fn observe<B: PasteboardBackend>(&mut self, backend: &mut B, force: bool) -> Option<HostClip> {
        self.observed_suppressed_change = false;
        let count = backend.change_count();
        if !force && count == self.last_count {
            return None;
        }
        let changed = self.last_count != -1 && count != self.last_count;
        self.last_count = count;
        let text = backend.get_string().filter(|text| !text.is_empty());
        let image = if self.images {
            backend.get_image().and_then(host_image)
        } else {
            None
        };
        if text.is_none() && image.is_none() {
            // A changed owner is newer even when no transferable content exists.
            // Preserve initial empty-startup behavior and never send a clear.
            self.last_seen = None;
            self.last_image = None;
            self.last_oversized = false;
            self.observed_suppressed_change = changed && !force;
            return None;
        }
        let digest = match &image {
            Some(HostImage::Png(png)) => Some(png.digest),
            _ => None,
        };
        // A too-large image is reported once per copy so its skip is logged,
        // and so is its going away (the copy that replaced it).
        let oversized = matches!(image, Some(HostImage::TooLarge { .. }));
        let new_image = digest != self.last_image || oversized || self.last_oversized;
        if !force && self.last_seen == text && !new_image {
            self.observed_suppressed_change = true;
            return None;
        }
        self.last_seen = text.clone();
        self.last_image = digest;
        self.last_oversized = oversized;
        Some(HostClip { text, image })
    }
}

#[path = "host_pasteboard_write.rs"]
mod write;
