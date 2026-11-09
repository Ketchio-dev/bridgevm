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
    /// A PUT held for its marks goes out once they are on disk (share_state.rs).
    pub(in crate::agent_console) fn release_held_share_put(&mut self, now: Instant) {
        if let Some(req) = self.share.as_mut().and_then(|share| share.release_held_put(now)) {
            self.queue.push_back(req);
        }
    }

    pub(in crate::agent_console) fn enqueue_one_share_host_change(&mut self, now: Instant) -> bool {
        self.enqueue_one_share_host_change_with(now, read_share_host_snapshot)
    }

    pub(in crate::agent_console) fn enqueue_one_share_host_change_with(
        &mut self,
        now: Instant,
        mut read: impl FnMut(&mut ShareState, &str, u64) -> Result<HostSnapshot, ShareFileReadError>,
    ) -> bool {
        let streams = self.share_big;
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        share.flush_sync_state(now);
        if share.held_put.is_some() {
            return true; // released by release_held_share_put once its marks are on disk
        }
        let fresh = share
            .host_actions_at
            .is_some_and(|at| now.duration_since(at) < share.interval);
        // A new plan from a real walk needs no revalidation; one from a kept walk
        // (share_host_watch.rs) is checked as a cached plan is, against that walk's start.
        let mut walked = false;
        if share.host_actions.is_empty() || !fresh {
            share.host_actions.clear();
            // An unmounted or renamed root is not the user deleting every file.
            match scan_share_host_dir(share, now) {
                Some(real) => walked = real,
                None => return false,
            }
            let actions = share
                .engine
                .on_host_scan_normalized(share.host_scan_scratch.drain(..));
            share.host_actions.extend(actions);
            share.host_actions_at = Some(now);
        }
        // Files found already synced are read, not sent: a bounded amount per step.
        let mut adopted_bytes = 0usize;
        while let Some(action) = share.host_actions.front() {
            // A name added, removed or renamed in the action's folder since the scan (a Windows
            // twin, an undo) can change what the scan decided: plan again from a new scan, a real
            // walk even if FSEvents has not reported the change yet (share_host_watch.rs).
            if !walked && folder_changed_since(&share.host_dir, action, share.host_actions_scanned) {
                share.host_actions.clear();
                share.forget_host_walk();
                return self.enqueue_one_share_host_change_with(now, read);
            }
            let action = share.host_actions.pop_front().expect("front was Some");
            match action {
                SyncAction::Get { name } => {
                    if share.engine.waits_for_first_listing(&name) {
                        continue;
                    }
                    let snapshot = match read_share_snapshot_or_skip(share, &name, streams, &mut read) {
                        None => continue,
                        Some(Ok(snapshot)) => snapshot,
                        // Larger than one range: streamed from the file (share_big_put.rs).
                        Some(Err(_)) => match start_big_put(share, &name, now) {
                            BigPutStart::Started => {
                                if let Some(req) = share.after_marks(ServiceReq::ShareBigPut(BigPutStep::Beg)) {
                                    self.queue.push_back(req);
                                }
                                return true;
                            }
                            BigPutStart::Adopted => continue,
                            BigPutStart::No => continue,
                        },
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
                        // The guest truncates before it writes (share_synced.rs).
                        share.mark_put(&name, now);
                        let req = share_put_req(name, push.bytes, push.hash, Some(snapshot.mtime_ms));
                        if let Some(req) = share.after_marks(req) {
                            self.queue.push_back(req);
                        }
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

/// True when a folder on the action's path, the share root included, changed
/// after `scanned` (or is gone). A Windows twin of the path needs a new name in
/// one of them: `foo.` beside `foo`, or `X` beside `x` for `x/foo`.
fn folder_changed_since(root: &Path, action: &SyncAction, scanned: std::time::SystemTime) -> bool {
    let (SyncAction::Get { name } | SyncAction::DeleteGuest { name }) = action else {
        return false;
    };
    let changed = |folder: &str| {
        std::fs::metadata(root.join(folder))
            .and_then(|meta| meta.modified())
            .map_or(true, |modified| modified >= scanned)
    };
    changed("") || name.match_indices('/').any(|(end, _)| changed(&name[..end]))
}

#[cfg(test)]
#[path = "share_sync_service_tests.rs"]
mod sync_service_tests;
