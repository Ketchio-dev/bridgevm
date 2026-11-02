//! Refused directory snapshots complete the wire slot without planning partial deletion.
#![cfg(test)]

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

fn synced_share() -> AgentConsoleHarness {
    let mut h = harness();
    let mut engine = ShareSync::new(1024);
    engine.on_guest_file("keep.txt".into(), vec![7], Some("m1"));
    engine.on_host_write_succeeded("keep.txt", Some(1));
    h.share = Some(ShareState {
        engine,
        host_dir: PathBuf::from("/unused-test-root"),
        guest_dir: "C:\\share".into(),
        interval: Duration::from_secs(1),
        last_poll: None,
        host_skip_seen: HashSet::new(),
        guest_ls_scratch: Vec::new(),
        host_scan_scratch: Vec::new(),
    });
    h
}

fn reply(h: &mut AgentConsoleHarness, payload: &[u8]) {
    let now = Instant::now();
    h.in_flight = Some((ServiceReq::ShareLs, now));
    h.handle_service_reply(&format!("LSOK {}", base64_encode(payload)), None, None, now);
    assert!(h.in_flight.is_none());
}

#[test]
fn rejected_listings_preserve_sync_state_and_recover_on_a_valid_snapshot() {
    let too_many: String = (0..=share_sync::MAX_LISTING_ENTRIES)
        .map(|i| format!("f{i}|1|0|m1\n"))
        .collect();
    let refused = [
        vec![b'x'; share_sync::MAX_LISTING_BYTES + 1],
        vec![b'x'; share_sync::MAX_LISTING_BYTES + 3],
        vec![0xff],
        too_many.into_bytes(),
    ];
    for payload in refused {
        let mut h = synced_share();
        h.queue.push_back(ServiceReq::Ping);
        reply(&mut h, &payload);
        assert_eq!(h.queue.len(), 1, "refused snapshot queued file mutations");
        assert!(matches!(h.queue.pop_front(), Some(ServiceReq::Ping)));
        reply(&mut h, b"keep.txt|1|0|m1\n");
        assert!(
            h.queue.is_empty(),
            "refused snapshot changed the sync baseline"
        );
        reply(&mut h, b"");
        assert!(
            matches!(h.queue.pop_front(), Some(ServiceReq::ShareDel { name, direction: ShareDelDirection::GuestToHost }) if name == "keep.txt"),
            "a genuine empty snapshot no longer propagates deletion"
        );
    }
}
