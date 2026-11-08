//! Resident GET completion is bound to both its active request and wire path.

use super::*;
#[path = "share_get_publish.rs"]
mod publish;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_get_reply(
        &mut self,
        name: &str,
        line: &str,
        now: Instant,
    ) {
        if let Some(rest) = line.strip_prefix("GETEND ") {
            if self.handle_share_get_end(name, rest, now) {
                self.complete_in_flight();
            }
        } else if line.starts_with("ERR") {
            self.get_accum = None;
            println!("BVAGENT SHARE get-error {name} {line}");
            self.complete_in_flight();
        }
    }

    pub(in crate::agent_console) fn handle_share_get_end(
        &mut self,
        name: &str,
        _rest: &str,
        now: Instant,
    ) -> bool {
        if !matches!(
            self.in_flight.as_ref(),
            Some((ServiceReq::ShareGet { name: active }, _)) if active == name
        ) {
            return false;
        }
        let Some(share) = self.share.as_ref() else {
            return false;
        };
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        if !self
            .get_accum
            .as_ref()
            .is_some_and(|accum| accum.path == self.share_guest_path_scratch)
        {
            return false;
        }
        let Some(finished) = self.take_finished_get() else {
            return false;
        };
        self.apply_finished_share_get(name, finished, now);
        true
    }
}
