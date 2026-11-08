//! One host step of the resident share service: record Mac files the guest
//! already holds (share_adopt.rs), then queue at most one PUT or guest
//! deletion, otherwise let the caller queue the guest listing.
//!
//! One scan serves a whole run of steps. A step sends one request, and a scan
//! per step made a first sync of N files cost N scans of N files: with 2000
//! files vCPU 0, whose thread runs this service, spent 80 % of its time in
//! lstat and readdir. The scan is redone once its actions run out or are a
//! poll interval old, so a Mac edit made meanwhile waits no longer than it
//! does while the share is idle. A cached PUT reads the file afresh, and a
//! cached guest deletion is dropped when the file is back on the Mac.

use super::*;

/// About a tenth of a second of reading and hashing per service step.
const SHARE_ADOPT_STEP_BYTES: usize = 64 << 20;

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn enqueue_one_share_host_change(&mut self, now: Instant) -> bool {
        self.enqueue_one_share_host_change_with(now, read_share_host_snapshot)
    }

    pub(in crate::agent_console) fn enqueue_one_share_host_change_with(
        &mut self,
        now: Instant,
        mut read: impl FnMut(&mut ShareState, &str) -> Result<HostSnapshot, ShareFileReadError>,
    ) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        let fresh = share
            .host_actions_at
            .is_some_and(|at| now.duration_since(at) < share.interval);
        if share.host_actions.is_empty() || !fresh {
            share.host_actions.clear();
            // An unmounted or renamed root is not the user deleting every file.
            if !scan_share_host_dir(share) {
                return false;
            }
            let actions = share
                .engine
                .on_host_scan_normalized(share.host_scan_scratch.drain(..));
            share.host_actions.extend(actions);
            share.host_actions_at = Some(now);
        }
        // Files found already synced are read, not sent: a bounded amount per step.
        let mut adopted_bytes = 0usize;
        while let Some(action) = share.host_actions.pop_front() {
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
                    // The scan may be older than this step: a file back on the Mac (an undo)
                    // keeps its guest copy, and the next scan sees it again.
                    if std::fs::symlink_metadata(share.host_dir.join(&name)).is_ok() {
                        continue;
                    }
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
