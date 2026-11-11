//! Guest-initiated writes/deletes use pinned, no-follow directory descriptors.

use super::guest_write_refusal::is_persistent;
use super::host_destination::Destination;
use super::*;

#[path = "share_host_delete.rs"]
mod delete;
#[path = "share_get_reply.rs"]
mod get_reply;
#[cfg(test)]
#[path = "share_get_test_fixture.rs"]
pub(in crate::agent_console) mod get_test_fixture;

/// Part of the host pass's setup (share_idle.rs).
pub(in crate::agent_console) fn forget_absent_share_failures(share: &mut ShareState) {
    (share.engine.for_pass()).forget_absent_failed(|name| Destination::absent(&share.host_dir, name));
}
