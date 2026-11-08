//! Guest write admission and self-write suppression, unchanged by read handling.

use super::super::PngImage;
use super::{html_digest, PasteboardBackend, Poller};
use crate::agent_console::clip_guest_admission::GUEST_CLIP_MAX_BYTES;
use std::path::PathBuf;

impl Poller {
    /// Write guest text to the pasteboard, recording the post-write changeCount so the write
    /// does not re-emerge from on_tick. Text over GUEST_CLIP_MAX_BYTES is never written. A
    /// changeCount that moved since the last observation is a newer Mac copy: it wins, is left
    /// in place and is reported by the next on_tick. (A copy landing between that check and
    /// set_string, or between set_string and change_count, is still missed: NSPasteboard has
    /// no compare-and-swap.)
    pub fn apply_set<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str) {
        if !self.owns_pasteboard(backend) {
            return;
        }
        if text.len() <= GUEST_CLIP_MAX_BYTES && backend.set_string(text) {
            self.empty_rereads = 0;
            self.last_seen = Some(text.to_owned());
            self.last_image = None;
            self.last_oversized = false;
            self.last_html = None;
            self.last_count = backend.change_count();
        }
    }

    /// The rich form of apply_set: the guest's text and its HTML in one write,
    /// under the same newer-Mac-copy rule and echo suppression.
    pub fn apply_set_html<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str, html: &str) {
        if !self.owns_pasteboard(backend) {
            return;
        }
        if text.len() <= GUEST_CLIP_MAX_BYTES && backend.set_text_html(text, html) {
            self.empty_rereads = 0;
            self.last_seen = Some(text.to_owned());
            self.last_image = None;
            self.last_oversized = false;
            self.last_html = Some(html_digest(html));
            self.last_count = backend.change_count();
        }
    }

    /// The image form of apply_set: one write of the guest's text (when given)
    /// and PNG, under the same newer-Mac-copy rule and echo suppression.
    pub fn apply_set_image<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
        png: &PngImage,
        text: Option<&str>,
    ) {
        if !self.owns_pasteboard(backend) {
            return;
        }
        let text = text.filter(|text| !text.is_empty() && text.len() <= GUEST_CLIP_MAX_BYTES);
        if backend.set_image(&png.bytes, text) {
            self.empty_rereads = 0;
            self.last_seen = text.map(str::to_owned);
            self.last_image = Some(png.digest);
            self.last_oversized = false;
            self.last_html = None;
            self.last_count = backend.change_count();
        }
    }

    /// The files form of apply_set: the guest's copied items as file URLs,
    /// under the same newer-Mac-copy rule and echo suppression. The URLs carry
    /// no text or image, so the pasteboard then reads as empty.
    pub fn apply_set_files<B: PasteboardBackend>(&mut self, backend: &mut B, files: &[PathBuf]) {
        if !self.owns_pasteboard(backend) {
            return;
        }
        if backend.set_files(files) {
            self.empty_rereads = 0;
            self.last_seen = None;
            self.last_image = None;
            self.last_oversized = false;
            self.last_html = None;
            self.last_count = backend.change_count();
        }
    }

    /// False when the Mac copied since the last observation. That copy wins:
    /// forget the last report so the next on_tick re-reports it even if it is
    /// unchanged.
    fn owns_pasteboard<B: PasteboardBackend>(&mut self, backend: &mut B) -> bool {
        if self.last_count != -1 && backend.change_count() != self.last_count {
            self.last_seen = None;
            self.last_image = None;
            self.last_oversized = false;
            self.last_html = None;
            return false;
        }
        true
    }
}
