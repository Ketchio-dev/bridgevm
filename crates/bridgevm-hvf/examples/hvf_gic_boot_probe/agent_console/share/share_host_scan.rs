//! A replaced or unavailable root is not a snapshot of host deletions.
//! Identity checks bracket the path-based walk; concurrent swaps inside that
//! interval and races below the selected root are not an atomic snapshot.

use super::*;
#[path = "share_host_scan_walk.rs"]
mod walk;

/// False when the bound root is unavailable, replaced, or incompletely listed.
pub(in crate::agent_console) fn scan_share_host_dir(share: &mut ShareState) -> bool {
    if !share.root_matches() {
        return false;
    }
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
    if !listed || !share.root_matches() {
        return false;
    }
    share_host_write::forget_absent_share_failures(share);
    std::mem::swap(&mut share.host_scan_scratch, &mut share.host_scan_work);
    share.engine.set_host_unseen(unseen);
    true
}
