//! Pasteboard observation and self-write suppression without native API access.

use super::PasteboardBackend;

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
    pub(super) observed_recopy: bool,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // -1 sentinel: the first on_tick always reads the pasteboard, so
            // startup content is emitted once (host wins at startup).
            last_count: -1,
            last_seen: None,
            observed_recopy: false,
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
        self.observed_recopy = false;
        let count = backend.change_count();
        if !force && count == self.last_count {
            return None;
        }
        self.last_count = count;
        let text = backend.get_string()?;
        if text.is_empty() {
            return None;
        }
        if !force && self.last_seen.as_deref() == Some(text.as_str()) {
            self.observed_recopy = true;
            return None;
        }
        self.last_seen = Some(text.clone());
        Some(text)
    }

    /// Write guest text to the pasteboard, recording the post-write changeCount
    /// so the write does not re-emerge from on_tick. A changeCount that moved
    /// since the last observation is a newer Mac copy: it wins, is left in
    /// place and is reported by the next on_tick. (A copy landing between that
    /// check and set_string, or between set_string and change_count, is still
    /// missed: NSPasteboard has no compare-and-swap.)
    pub fn apply_set<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str) {
        if self.last_count != -1 && backend.change_count() != self.last_count {
            // Forget the last report so the next on_tick re-reports the Mac
            // text even if it is unchanged; the Mac copy then wins.
            self.last_seen = None;
            return;
        }
        if backend.set_string(text) {
            self.last_seen = Some(text.to_owned());
            self.last_count = backend.change_count();
        }
    }
}
