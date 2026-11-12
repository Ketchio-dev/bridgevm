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
            println!("BVAGENT SHARE del-skipped {name} share-disabled");
            return;
        };
        let Ok(root) = share.checked_root() else {
            println!("BVAGENT SHARE del-deferred <root-unavailable>");
            return;
        };
        share.forget_host_walk();
        let expected = share.engine.delete_host_stat(name);
        match Destination::from_root(root, name, false).and_then(|d| match expected {
            Some(expected) => d.delete_unless_changed(expected),
            None => {
                println!("BVAGENT SHARE del-unchecked {name} host-version-unknown");
                d.delete()
            }
        }) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if super::host_destination::is_mac_changed(&e) => {
                share.engine.on_host_copy_kept(name);
                println!("BVAGENT SHARE del-skipped {name} mac-changed");
                return;
            }
            Err(e) => {
                println!("BVAGENT SHARE del-refused {name} {e}");
                return;
            }
        }
        share.engine.on_host_deleted(name);
        println!("BVAGENT SHARE del guest->host {name} t={}", self.t_ms(now));
    }
}
