//! The desktop switch and display scale as resident agent verbs.
//!
//! `DISPLAYMODE <w> <h>` and `DISPLAYSCALE <p>` run the C# that
//! desktop_switch.rs and desktop_scale.rs otherwise send as RUN powershell
//! lines, compiled once in the agent process (bvagent.ps1 Invoke-BvDisplay).
//! Each RUN paid a process start and a C# compile, about 0.7 s of a live
//! resize, twice with a scale. The agent answers as for a RUN, `OUT 0
//! <b64(BVDISPLAY|BVDPI line)>`, so run.log gets the same lines.
//!
//! The queue holds the verb, and its wire line is chosen when it is sent. An
//! agent without the verbs answers `OUT 255 <b64(unknown token: ...)>`
//! (bvagent.ps1's default branch), and one that could not compile or run them
//! `ERR <verb> ...`. That request is queued again in front, and it and every
//! later display request go as RUN until the agent's next READY. A size the
//! verb does not take always goes as RUN.

use std::ops::RangeInclusive;

use super::desktop_scale::{desktop_scale_run_command, DESKTOP_SCALES};
use super::desktop_switch::desktop_switch_run_command;
use super::*;

pub(super) const MODE_VERB: &str = "DISPLAYMODE";
pub(super) const SCALE_VERB: &str = "DISPLAYSCALE";
/// The sides bvagent.ps1 takes in `DISPLAYMODE`.
const VERB_SIDES: RangeInclusive<u32> = 320..=16384;

/// The RUN powershell line a display verb goes as: once the agent refused the
/// verbs, and for a size the verb does not take. None for a verb that goes as
/// itself and for every other command.
pub(super) fn run_command(command: &str, refused: bool) -> Option<String> {
    let mut fields = command.split(' ');
    let verb = fields
        .next()
        .filter(|verb| [MODE_VERB, SCALE_VERB].contains(verb))?;
    let values: Vec<u32> = fields.map(str::parse).collect::<Result<_, _>>().ok()?;
    match (verb, values.as_slice()) {
        (MODE_VERB, &[width, height]) => {
            (refused || !VERB_SIDES.contains(&width) || !VERB_SIDES.contains(&height))
                .then(|| desktop_switch_run_command(width, height))
        }
        (SCALE_VERB, &[percent]) if DESKTOP_SCALES.contains(&percent) => {
            refused.then(|| desktop_scale_run_command(percent))
        }
        _ => None,
    }
}

impl AgentConsoleHarness {
    /// Whether `line` refuses the display verb `command` went to the agent as.
    /// The request is then queued again in front, to go as RUN. A command that
    /// went as RUN, and one with no RUN form (a malformed control-file line),
    /// take the usual reply handling.
    pub(super) fn display_verb_refused(&mut self, line: &str, command: &str, now: Instant) -> bool {
        if run_command(command, self.display_verbs_refused).is_some()
            || run_command(command, true).is_none()
        {
            return false;
        }
        let verb = command.split(' ').next().unwrap_or("");
        let refusal = match parse_out_line(line) {
            Some((255, output)) => base64_decode(output)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|_| line.to_string()),
            Some(_) => return false,
            None => {
                let error = line
                    .strip_prefix("ERR ")
                    .and_then(|rest| rest.strip_prefix(verb));
                if !error.is_some_and(|rest| rest.is_empty() || rest.starts_with(' ')) {
                    return false;
                }
                line.to_string()
            }
        };
        println!(
            "BVAGENT {command} -> {refusal}; display requests go as RUN powershell until the agent restarts t={}",
            self.t_ms(now)
        );
        self.display_verbs_refused = true;
        if let Some((request, _)) = self.in_flight.take() {
            self.queue.push_front(request);
        }
        true
    }
}

#[cfg(test)]
#[path = "display_verbs_tests.rs"]
mod tests;
