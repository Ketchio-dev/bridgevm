//! Pasteboard observation and self-write suppression without native API access.

use super::PasteboardBackend;

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
    pub(super) observed_suppressed_change: bool,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // -1 sentinel: the first on_tick always reads the pasteboard, so
            // startup content is emitted once (host wins at startup).
            last_count: -1,
            last_seen: None,
            observed_suppressed_change: false,
        }
    }

    /// One poll step: Some(text) iff the pasteboard changed externally to a
    /// non-empty text we haven't already emitted or set ourselves.
    pub fn on_tick<B: PasteboardBackend>(&mut self, backend: &mut B) -> Option<String> {
        self.observe(backend, false)
    }

    pub(super) fn report_current<B: PasteboardBackend>(
        &mut self,
        backend: &mut B,
    ) -> Option<String> {
        self.observe(backend, true)
    }

    fn observe<B: PasteboardBackend>(&mut self, backend: &mut B, force: bool) -> Option<String> {
        self.observed_suppressed_change = false;
        let count = backend.change_count();
        if !force && count == self.last_count {
            return None;
        }
        let changed = self.last_count != -1 && count != self.last_count;
        self.last_count = count;
        let Some(text) = backend.get_string().filter(|text| !text.is_empty()) else {
            // A changed owner is newer even when no transferable text exists.
            // Preserve initial empty-startup behavior and never send a clear.
            self.last_seen = None;
            self.observed_suppressed_change = changed && !force;
            return None;
        };
        if !force && self.last_seen.as_deref() == Some(text.as_str()) {
            self.observed_suppressed_change = true;
            return None;
        }
        self.last_seen = Some(text.clone());
        Some(text)
    }

}

#[path = "host_pasteboard_write.rs"]
mod write;
