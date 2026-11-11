//! A replaced or unavailable root is not a snapshot of host deletions.
//! Identity checks bracket the path-based walk; concurrent swaps inside that
//! interval and races below the selected root are not an atomic snapshot.

use super::*;
#[path = "share_host_scan_accept.rs"]
mod accept;
#[path = "share_host_scan_walk.rs"]
mod walk;

/// A scan of the Mac folder for the host pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agent_console) enum HostScan {
    /// The folder was walked.
    Walked,
    /// The kept walk is planned from again.
    Kept,
    /// The kept walk's last plan was idle and stands (share_idle.rs): no pass.
    Unchanged,
}

/// None when the bound root is unavailable, replaced, or incompletely listed.
pub(in crate::agent_console) fn scan_share_host_dir(share: &mut ShareState, now: Instant) -> Option<HostScan> {
    if !share.root_matches() {
        return None;
    }
    // Nothing below the root changed since the last complete walk (share_host_watch.rs).
    if let Some(walk) = share.kept_host_walk(now) {
        if share.host_plan_unchanged(walk) {
            share.check_skipped_host_pass(&|name: &str| host_destination::Destination::absent(&share.host_dir, name));
            share.report_root_state(false);
            return Some(HostScan::Unchanged);
        }
        let unseen = share.reuse_host_walk();
        return accept::accept_scan(share, true, unseen).then_some(HostScan::Kept);
    }
    #[cfg(test)]
    {
        share.host_scans += 1;
    }
    let changes = share.begin_host_walk();
    share.host_actions_scanned = std::time::SystemTime::now();
    share.host_scan_work.clear();
    let mut unseen = Vec::new();
    let listed = walk::scan_share_host_dir_inner(
        &share.host_dir,
        &share.host_dir,
        share.engine.max_bytes(),
        &mut share.host_skip_seen,
        &mut share.host_scan_work,
        &mut unseen,
    );
    if listed {
        share.keep_host_walk(changes, &unseen, now);
    }
    let accepted = accept::accept_scan(share, listed, unseen);
    if !accepted {
        share.forget_host_walk();
    }
    accepted.then_some(HostScan::Walked)
}
