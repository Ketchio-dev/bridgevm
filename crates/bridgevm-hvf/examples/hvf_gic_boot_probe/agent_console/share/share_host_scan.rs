//! A replaced or unavailable root is not a snapshot of host deletions.
//! Identity checks bracket the path-based walk; concurrent swaps inside that
//! interval and races below the selected root are not an atomic snapshot.

use super::*;
#[path = "share_host_scan_accept.rs"]
mod accept;
#[path = "share_host_scan_walk.rs"]
mod walk;

/// False when the bound root is unavailable, replaced, or incompletely listed.
pub(in crate::agent_console) fn scan_share_host_dir(share: &mut ShareState, now: Instant) -> bool {
    if !share.root_matches() {
        return false;
    }
    // Nothing below the root changed since the last complete walk (share_host_watch.rs).
    if let Some(unseen) = share.reuse_host_walk(now) {
        return accept::accept_scan(share, true, unseen);
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
    accepted
}
