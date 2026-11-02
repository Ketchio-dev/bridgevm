//! Failure-budget exhaustion stops share work, not unrelated console service.

use super::*;
use crate::agent_console::share::share_host_write::get_test_fixture::fetched;
#[path = "share_failure_fixture.rs"]
mod fixture;

#[test]
fn failed_destination_limit_disables_share_and_drops_queued_puts_and_deletes() {
    let (mut h, _root) = fixture::nearly_exhausted_share();
    h.queue.push_back(ServiceReq::ShareLs);
    h.queue.push_back(share_put_req("a".into(), vec![1], 1));
    h.queue.push_back(ServiceReq::ShareDel { name: "a".into(), direction: ShareDelDirection::HostToGuest });
    h.queue.push_back(ServiceReq::Ping);
    // A real root verifies, then the invalid component exhausts the same budget.
    fetched(&mut h, "../refused", &[1]);
    assert!(h.share.is_none());
    assert_eq!(h.queue.len(), 1);
    assert!(matches!(h.queue.front(), Some(ServiceReq::Ping)));
}
