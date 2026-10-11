//! Terminal replies that only a raw protocol verb produces.

use super::*;

/// Classify a reply to a raw-verb request before the generic reply rules.
/// Control-file requests are never retransmitted, so a terminal reply this
/// misses would hold the lockstep wire slot until the guest agent restarts.
pub(super) fn handle_verb_reply(line: &str, command: &str) -> Option<ReplyProgress> {
    // PONG answers only PING; a stray one must not complete a shell command.
    if line == "PONG" && command.split_whitespace().next() == Some("PING") {
        println!("BVAGENT {command} -> PONG");
        return Some(ReplyProgress::Complete);
    }
    // An agent that lacks a verb answers `OUT 255 unknown token: ...`; the
    // generic OUT rule reports it as a command result and completes it.
    if parse_out_line(line).is_some_and(|(exit, _)| exit == 255) {
        return None;
    }
    handle_window_reply(line, command)
}
