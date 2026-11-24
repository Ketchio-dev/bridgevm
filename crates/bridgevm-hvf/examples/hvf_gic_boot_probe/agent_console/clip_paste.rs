//! Legacy (non-ordered) text entry sends non-ASCII through the guest
//! clipboard. A long paste goes over the existing guest CLIPSET verb with a
//! relay-owned request ID. Either command emits BVPASTE_READY before Ctrl+V.
//! The clipboard change belongs to the app's paste, so the
//! next CLIPGET re-learns the baseline instead of publishing the typed text
//! (possibly a password) to the Mac pasteboard and Universal Clipboard.

use super::*;

/// Prefix of HvfClipboardPaste.swift's `marker`, which its command echoes.
/// clip_paste_tests.rs fails if the Swift source stops emitting it.
pub(super) const LEGACY_PASTE_MARKER: &str = "BVPASTE_READY ";
const CLIPPASTE: &str = "CLIPPASTE ";

pub(super) fn is_legacy_paste(command: &str) -> bool {
    command.contains(LEGACY_PASTE_MARKER)
}

fn paste_arguments(args: &str) -> Option<(&str, &str)> {
    let (id, payload) = args.split_once(' ')?;
    (valid_request_id(id) && !payload.is_empty()).then_some((id, payload))
}

// Only the matching clipboard success can arm Ctrl+V. The service already
// keeps one request on the guest wire, so the ID never needs to go to Windows.
pub(super) fn clip_paste_reply(command: &str, line: &str) -> Option<ReplyProgress> {
    let args = command.strip_prefix(CLIPPASTE)?;
    let Some((id, _)) = paste_arguments(args) else {
        return Some(ReplyProgress::Ignored);
    };
    let marker = format!("{LEGACY_PASTE_MARKER}{id}");
    let reply = match line {
        "OK CLIPSET" => super::harness_protocol::command_output_text(&marker, 0, &marker),
        error if error.starts_with("ERR ") => super::harness_protocol::command_output_text(&marker, 1, ""),
        _ => return Some(ReplyProgress::Ignored),
    };
    println!("{reply}");
    Some(ReplyProgress::Complete)
}

impl AgentConsoleHarness {
    /// Encode a control command. Marking at write time covers a reply, a failure,
    /// and an agent restart (READY) that drops it after Set-Clipboard ran; a guest
    /// child outliving that restart can still land late (not covered). A later
    /// CLIPSET OK re-advances the baseline, since that host copy overwrote it.
    pub(super) fn write_ctl_wire_line(&mut self, command: &str) -> bool {
        if let Some(args) = command.strip_prefix(CLIPPASTE) {
            let Some((_, payload)) = paste_arguments(args) else {
                eprintln!("BVAGENT CLIPPASTE invalid request");
                return false;
            };
            self.last_synced.rebaseline = true;
            let _ = writeln!(self.service_line_scratch, "CLIPSET {payload}");
            return true;
        }
        // PRINTWATCH waits for an agent that has it (print_jobs.rs).
        if !self.note_print_watch(command) {
            return false;
        }
        // A display verb the agent refused, or cannot take, goes as its RUN line.
        match super::display_verbs::run_command(command, self.display_verbs_refused) {
            Some(run) => write_command_wire_line_into(&run, &mut self.service_line_scratch),
            None => write_command_wire_line_into(command, &mut self.service_line_scratch),
        }
        if is_legacy_paste(command) {
            self.last_synced.rebaseline = true;
        }
        // A restarted agent is asked again (guest_link.rs).
        self.link_watch |= command == super::guest_link::WATCH_VERB;
        true
    }
}
