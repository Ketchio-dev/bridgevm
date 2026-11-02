//! Publish an accepted scan, then announce recovery from a root deferral.

use super::*;

pub(super) fn accept_scan(share: &mut ShareState, listed: bool, unseen: Vec<String>) -> bool {
    if !listed || !share.root_matches() {
        share.report_root_state(true);
        return false;
    }
    share_host_write::forget_absent_share_failures(share);
    std::mem::swap(&mut share.host_scan_scratch, &mut share.host_scan_work);
    share.engine.set_host_unseen(unseen);
    share.report_root_state(false);
    true
}
