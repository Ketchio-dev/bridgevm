//! Refused directory snapshots complete the wire slot without planning partial deletion.
#![cfg(test)]

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

fn synced_share() -> AgentConsoleHarness {
    let mut engine = ShareSync::new(1024);
    engine.on_guest_file("keep.txt".into(), vec![7], Some("m1"));
    engine.on_host_write_succeeded("keep.txt", Some(1));
    share_with(engine)
}

fn share_with(engine: ShareSync) -> AgentConsoleHarness {
    let mut h = harness();
    h.share = Some(ShareState::new(
        engine,
        PathBuf::from("/unused-test-root"),
        "C:\\share".into(),
        Duration::from_secs(1),
    ));
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
fn missing_folder_reply(h: &mut AgentConsoleHarness) {
    let now = Instant::now();
    h.in_flight = Some((ServiceReq::ShareLs, now));
    let line = r"ERR LSR Cannot find path 'C:\share\' because it does not exist.";
    h.handle_service_reply(line, None, None, now);
    assert!(h.in_flight.is_none());
}

fn powershell_script(command: &str) -> String {
    let b64 = command.rsplit(' ').next().unwrap();
    let utf16: Vec<u16> = base64_decode(b64)
        .unwrap()
        .chunks(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&utf16).unwrap()
}

#[test]
fn a_missing_guest_folder_is_created_once_and_only_before_anything_synced() {
    let mut fresh = share_with(ShareSync::new(1024));
    missing_folder_reply(&mut fresh);
    let Some(ServiceReq::Ctl(command)) = fresh.queue.pop_front() else {
        panic!("a failed listing with nothing synced did not create the guest folder");
    };
    let script = powershell_script(&command);
    assert!(script.contains("CreateDirectory"), "{script}");
    assert!(script.contains(&base64_encode(b"C:\\share")), "{script}");
    missing_folder_reply(&mut fresh);
    assert!(fresh.queue.is_empty(), "the folder is created once per session");

    // Recreated empty, a folder that held synced files would empty the Mac one.
    let mut synced = synced_share();
    missing_folder_reply(&mut synced);
    assert!(synced.queue.is_empty(), "a synced share recreated the guest folder");
}

#[path = "share_listing_malformed_tests.rs"]
mod malformed_listing;
