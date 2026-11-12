//! Recheck the selected root before a host deletion becomes a guest DEL.

use super::*;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn write_share_delete_wire(&mut self, name: &str) -> bool {
        let Some(share) = self.share.as_mut() else {
            println!("BVAGENT SHARE del-skipped {name} share-disabled");
            return false;
        };
        if !share.root_matches() {
            println!("BVAGENT SHARE del-skipped {name} root-unavailable");
            return false;
        }
        if std::fs::symlink_metadata(share.host_dir.join(name)).is_ok() {
            println!("BVAGENT SHARE del-skipped {name} mac-copy-returned");
            return false;
        }
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        self.service_line_scratch.push_str("DEL ");
        base64_encode_into(
            self.share_guest_path_scratch.as_bytes(),
            &mut self.service_line_scratch,
        );
        if self.delete_if_unchanged {
            if let Some((size, stamp)) = share.engine.delete_guest_stat(name) {
                use std::fmt::Write as _;
                let _ = write!(self.service_line_scratch, " {size} {stamp}");
            } else {
                println!("BVAGENT SHARE del-unchecked {name} guest-version-unknown");
            }
        }
        self.service_line_scratch.push('\n');
        true
    }
}
