//! Legacy (non-ordered) text entry sends non-ASCII through the guest
//! clipboard: HvfClipboardPaste.swift appends a PowerShell `Set-Clipboard`
//! command (appended to the control file) that prints `BVPASTE_READY <uuid>`, then sends
//! ctrl+v. That clipboard change is the app's paste, not a guest copy, so the
//! next CLIPGET re-learns the baseline instead of publishing the typed text
//! (possibly a password) to the Mac pasteboard and Universal Clipboard.

use super::*;

/// Prefix of HvfClipboardPaste.swift's `marker`, which its command echoes.
/// clip_paste_tests.rs fails if the Swift source stops emitting it.
pub(super) const LEGACY_PASTE_MARKER: &str = "BVPASTE_READY ";

pub(super) fn is_legacy_paste(command: &str) -> bool {
    command.contains(LEGACY_PASTE_MARKER)
}

impl AgentConsoleHarness {
    /// Encode a control command. Marking at write time covers a reply, a failure,
    /// and an agent restart (READY) that drops it after Set-Clipboard ran; a guest
    /// child outliving that restart can still land late (not covered). A later
    /// CLIPSET OK re-advances the baseline, since that host copy overwrote it.
    pub(super) fn write_ctl_wire_line(&mut self, command: &str) -> bool {
        write_command_wire_line_into(command, &mut self.service_line_scratch);
        if is_legacy_paste(command) {
            self.last_synced.rebaseline = true;
        }
        true
    }
}
