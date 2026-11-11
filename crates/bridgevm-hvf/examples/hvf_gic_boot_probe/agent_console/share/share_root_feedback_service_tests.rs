//! Root notices follow complete accepted host scans, not one identity match.

use super::*;

const BLOCKED: &str = "BVAGENT SHARE root-state blocked";
const READY: &str = "BVAGENT SHARE root-state ready";

fn missing_root_share(rig: &mut Rig) {
    std::fs::remove_dir(&rig.dir).unwrap();
    let _ = ShareState::take_root_notices();
    rig.h.share = Some(ShareState::new(
        ShareSync::new(512),
        rig.dir.clone(),
        ROOT.into(),
        Duration::from_millis(500),
    ));
    assert_eq!(ShareState::take_root_notices(), [BLOCKED], "before any scan");
}

#[test]
fn an_initially_missing_root_reports_once_and_recovers_after_an_accepted_scan() {
    let mut rig = Rig::new("root-notice-missing");
    missing_root_share(&mut rig);
    rig.run(12);
    assert!(ShareState::take_root_notices().is_empty());
    std::fs::create_dir(&rig.dir).unwrap();
    std::fs::write(rig.dir.join("a.txt"), b"after").unwrap();
    rig.run(12);
    assert_eq!(ShareState::take_root_notices(), [READY]);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"after"[..]));
    rig.assert_settled();
    assert!(ShareState::take_root_notices().is_empty());
}

/// The drive plugged in while Windows still boots: the first scan already finds the folder.
#[test]
fn a_root_missing_at_the_start_and_back_before_the_first_scan_reports_ready() {
    let mut rig = Rig::new("root-notice-early");
    missing_root_share(&mut rig);
    std::fs::create_dir(&rig.dir).unwrap();
    std::fs::write(rig.dir.join("a.txt"), b"early").unwrap();
    rig.run(12);
    assert_eq!(ShareState::take_root_notices(), [READY]);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"early"[..]));
    rig.assert_settled();
}

/// The folder came back between the app's check at Start (its missing-folder notice) and the
/// runtime's start: the runtime says `ready` at once, with no scan and no earlier `blocked`.
#[test]
fn a_root_present_when_the_runtime_starts_reports_ready_at_once() {
    let rig = Rig::new("root-notice-present");
    let _ = ShareState::take_root_notices();
    let _share = ShareState::new(ShareSync::new(512), rig.dir.clone(), ROOT.into(), Duration::from_millis(500));
    assert_eq!(ShareState::take_root_notices(), [READY]);
}

#[test]
fn a_restored_root_or_delivered_delete_alone_keeps_the_notice_until_a_scan() {
    let mut rig = Rig::synced("root-notice-queued-delete", "a.txt", b"A");
    assert_eq!(ShareState::take_root_notices(), [READY], "the runtime's first word on a present root");
    std::fs::remove_file(rig.dir.join("a.txt")).unwrap();
    assert!(rig.h.enqueue_one_share_host_change(rig.now));
    let holder = OwnedHolder::new("notice-queued-delete");
    let original = holder.replace_root(&rig);
    for _ in 0..3 {
        assert!(pump(&mut rig).is_empty());
    }
    assert!(matches!(rig.h.queue.front(), Some(ServiceReq::ShareDel {
        name, direction: ShareDelDirection::HostToGuest
    }) if name == "a.txt"));
    assert_eq!(ShareState::take_root_notices(), [BLOCKED]);
    restore_root(&rig, &original);
    let sent = pump(&mut rig);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(ShareState::take_root_notices().is_empty());
    for reply in rig.guest.reply(&sent[0]) {
        rig.h.handle_service_reply(&reply, None, None, rig.now);
    }
    assert!(rig.guest.content("a.txt").is_none());
    assert!(ShareState::take_root_notices().is_empty());
    rig.run(12);
    assert_eq!(ShareState::take_root_notices(), [READY]);
    rig.assert_settled();
    assert!(ShareState::take_root_notices().is_empty());
}
