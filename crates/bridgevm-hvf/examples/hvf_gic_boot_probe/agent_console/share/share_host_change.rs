//! One host-scan step of the resident share service: queue at most one PUT or
//! guest deletion, otherwise let the caller queue the guest listing.

use super::*;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn enqueue_one_share_host_change(&mut self) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        // An unmounted or renamed root is not the user deleting every file.
        if !scan_share_host_dir(share) {
            return false;
        }
        for action in share
            .engine
            .on_host_scan_normalized(share.host_scan_scratch.drain(..))
        {
            match action {
                SyncAction::Get { name } => {
                    let path = share.host_dir.join(&name);
                    let bytes = match read_share_file_bounded(&path, share.engine.max_bytes()) {
                        Ok(bytes) => bytes,
                        Err(ShareFileReadError::TooLarge(size)) => {
                            if let Some(mtime_ms) = file_mtime_ms(&path) {
                                print_host_skip_once(
                                    share,
                                    &name,
                                    mtime_ms,
                                    HostSkipKind::TooLarge,
                                    size,
                                );
                            }
                            continue;
                        }
                        Err(ShareFileReadError::Io) => continue,
                    };
                    let Some(mtime_ms) = file_mtime_ms(&path) else {
                        continue;
                    };
                    if let Some(push) = share.engine.on_host_file(name.clone(), bytes, mtime_ms) {
                        self.queue
                            .push_back(share_put_req(name, push.bytes, push.hash));
                        return true;
                    }
                }
                SyncAction::DeleteGuest { name } => {
                    self.queue.push_back(ServiceReq::ShareDel {
                        name,
                        direction: ShareDelDirection::HostToGuest,
                    });
                    return true;
                }
                SyncAction::Skip { name, reason } => print_share_skip(&name, reason),
                SyncAction::DeleteHost { .. } => {}
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "share_sync_service_tests.rs"]
mod sync_service_tests;
