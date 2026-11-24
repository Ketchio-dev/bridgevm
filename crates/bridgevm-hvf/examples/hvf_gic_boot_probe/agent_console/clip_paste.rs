//! Legacy (non-ordered) text entry sends non-ASCII through the guest
//! clipboard. A long paste uses CLIPSET and verifies the clipboard with CLIPGET.
//! The relay emits BVPASTE_READY before Ctrl+V, and clipboard sync re-learns
//! the guest baseline without publishing the typed text to the Mac.

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

impl AgentConsoleHarness {
    pub(super) fn handle_clip_paste_reply(
        &mut self,
        command: &str,
        line: &str,
        send: &mut dyn FnMut(&[u8]),
    ) -> bool {
        let Some((id, payload)) = command.strip_prefix(CLIPPASTE).and_then(paste_arguments) else {
            return false;
        };
        if self.confirming_clip_paste {
            if let Some(clipboard) = line.strip_prefix("CLIP ") {
                self.finish_clip_paste(id, clipboard == payload);
            }
        } else if line == "OK CLIPSET" {
            // A retransmitted clipboard sync can leave a stale OK on the wire.
            // Check the bytes with a read that the guest processes after this write.
            self.confirming_clip_paste = true;
            send(b"CLIPGET\n");
        } else if line == "ERR CLIPSET" || line.starts_with("ERR CLIPSET ") {
            self.finish_clip_paste(id, false);
        }
        true
    }

    fn finish_clip_paste(&mut self, id: &str, ready: bool) {
        let marker = format!("{LEGACY_PASTE_MARKER}{id}");
        let output = if ready { marker.as_str() } else { "" };
        let reply = super::harness_protocol::command_output_text(&marker, i32::from(!ready), output);
        println!("{reply}");
        self.get_accum = None;
        self.complete_in_flight();
    }

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
            self.confirming_clip_paste = false;
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
