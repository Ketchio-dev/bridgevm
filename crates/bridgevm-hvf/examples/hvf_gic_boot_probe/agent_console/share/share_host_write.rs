//! Guest-initiated writes/deletes use pinned, no-follow directory descriptors.

use super::guest_write_refusal::is_persistent;
use super::host_destination::Destination;
use super::*;

#[path = "share_get_reply.rs"]
mod get_reply;
#[cfg(test)]
#[path = "share_get_test_fixture.rs"]
pub(in crate::agent_console) mod get_test_fixture;

pub(in crate::agent_console) fn forget_absent_share_failures(share: &mut ShareState) {
    share
        .engine
        .forget_absent_failed(|name| Destination::absent(&share.host_dir, name));
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_delete(
        &mut self,
        name: &str,
        direction: ShareDelDirection,
        now: Instant,
    ) {
        if !matches!(direction, ShareDelDirection::GuestToHost) {
            return;
        }
        let Some(share) = self.share.as_mut() else {
            return;
        };
        match Destination::open(&share.host_dir, name, false).and_then(|d| d.delete()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                println!("BVAGENT SHARE del-refused {name} {e}");
                return;
            }
        }
        share.engine.on_host_deleted(name);
        println!("BVAGENT SHARE del guest->host {name} t={}", self.t_ms(now));
    }
}
