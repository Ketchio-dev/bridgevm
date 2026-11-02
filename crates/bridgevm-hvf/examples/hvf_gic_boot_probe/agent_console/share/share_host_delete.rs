//! Guest deletion may mutate only a verified, held shared-root destination.

use super::*;

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
        let Ok(root) = share.checked_root() else {
            println!("BVAGENT SHARE del-deferred <root-unavailable>");
            return;
        };
        match Destination::from_root(root, name, false).and_then(|d| d.delete()) {
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
