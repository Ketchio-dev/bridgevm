//! One host-scan step of the resident share service: record Mac files the guest
//! already holds (share_adopt.rs), then queue at most one PUT or guest
//! deletion, otherwise let the caller queue the guest listing.

use super::*;

/// About a tenth of a second of reading and hashing per service step.
const SHARE_ADOPT_STEP_BYTES: usize = 64 << 20;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn enqueue_one_share_host_change(&mut self) -> bool {
        self.enqueue_one_share_host_change_with(read_share_host_snapshot)
    }

    pub(in crate::agent_console) fn enqueue_one_share_host_change_with(
        &mut self,
        mut read: impl FnMut(&mut ShareState, &str) -> Result<HostSnapshot, ShareFileReadError>,
    ) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        // An unmounted or renamed root is not the user deleting every file.
        if !scan_share_host_dir(share) {
            return false;
        }
        // Files found already synced are read, not sent: a bounded amount per step.
        let mut adopted_bytes = 0usize;
        for action in share
            .engine
            .on_host_scan_normalized(share.host_scan_scratch.drain(..))
        {
            match action {
                SyncAction::Get { name } => {
                    if share.engine.waits_for_first_listing(&name) {
                        continue;
                    }
                    let Some(snapshot) = read_share_snapshot_or_skip(share, &name, &mut read)
                    else {
                        continue;
                    };
                    if share.engine.adopt_listed_copy(&name, &snapshot.bytes, snapshot.mtime_ms) {
                        adopted_bytes += snapshot.bytes.len();
                        if adopted_bytes >= SHARE_ADOPT_STEP_BYTES {
                            return true;
                        }
                        continue;
                    }
                    if let Some(push) =
                        share
                            .engine
                            .on_host_file(name.clone(), snapshot.bytes, snapshot.mtime_ms)
                    {
                        self.queue
                            .push_back(share_put_req(
                            name,
                            push.bytes,
                            push.hash,
                            Some(snapshot.mtime_ms),
                        ));
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
