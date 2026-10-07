//! Failure-budget exhaustion stops share work, not unrelated console service.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

#[test]
fn failed_destination_limit_disables_share_and_drops_queued_puts_and_deletes() {
    let mut h = harness();
    let mut engine = ShareSync::new(1024);
    for i in 0..4095 {
        let name = format!("blocked/{i}");
        engine.on_guest_file(name.clone(), vec![1], Some("g1"));
        assert!(!engine.on_host_write_failed(&name));
    }
    h.share = Some(ShareState {
        engine,
        host_dir: PathBuf::from("/unused-root"),
        guest_dir: "C:\\share".into(),
        interval: Duration::from_secs(1),
        last_poll: None,
        host_skip_seen: HashSet::new(),
        guest_ls_scratch: Vec::new(),
        host_scan_scratch: Vec::new(),
    });
    h.queue.push_back(ServiceReq::ShareLs);
    h.queue.push_back(share_put_req("a".into(), vec![1], 1));
    h.queue.push_back(ServiceReq::ShareDel { name: "a".into(), direction: ShareDelDirection::HostToGuest });
    h.queue.push_back(ServiceReq::Ping);
    h.get_accum = Some(GetAccum { path: "ignored".into(), total: 1, nchunks: 1, bytes: vec![1], chunks_seen: 1 });
    // Invalid component refuses before opening even the root; no host mutation.
    h.handle_share_get_end("../refused", "", Instant::now());
    assert!(h.share.is_none());
    assert_eq!(h.queue.len(), 1);
    assert!(matches!(h.queue.front(), Some(ServiceReq::Ping)));
}
