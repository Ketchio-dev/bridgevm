//! Recheck the selected root before a host deletion becomes a guest DEL.

use super::*;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn write_share_delete_wire(&mut self, name: &str) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        if !share.root_matches() {
            return false;
        }
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        self.service_line_scratch.push_str("DEL ");
        base64_encode_into(
            self.share_guest_path_scratch.as_bytes(),
            &mut self.service_line_scratch,
        );
        self.service_line_scratch.push('\n');
        true
    }
}
