//! Guest write admission and self-write suppression, unchanged by read handling.

use super::{PasteboardBackend, Poller};
use crate::agent_console::clip_guest_admission::GUEST_CLIP_MAX_BYTES;

impl Poller {
    /// Write guest text to the pasteboard, recording the post-write changeCount so the write
    /// does not re-emerge from on_tick. Text over GUEST_CLIP_MAX_BYTES is never written. A
    /// changeCount that moved since the last observation is a newer Mac copy: it wins, is left
    /// in place and is reported by the next on_tick. (A copy landing between that check and
    /// set_string, or between set_string and change_count, is still missed: NSPasteboard has
    /// no compare-and-swap.)
    pub fn apply_set<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str) {
        if self.last_count != -1 && backend.change_count() != self.last_count {
            // Forget the last report so the next on_tick re-reports the Mac
            // text even if it is unchanged; the Mac copy then wins.
            self.last_seen = None;
            return;
        }
        if text.len() <= GUEST_CLIP_MAX_BYTES && backend.set_string(text) {
            self.last_seen = Some(text.to_owned());
            self.last_count = backend.change_count();
        }
    }
}
