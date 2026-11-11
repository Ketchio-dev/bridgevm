//! LSSAME (share_listing_same.rs): an unchanged guest folder is not sent again, the kept listing
//! is applied again as the same line was, and an agent without the marker gets the plain LSR.

use super::*;

const LSSAME_READY: &str = "GUEST v3-share2 lssame";

fn lsr(rig: &mut Rig) -> String {
    rig.h.service_req_line(&ServiceReq::ShareLs, rig.now).unwrap().to_owned()
}

fn root() -> String {
    base64_encode(ROOT.as_bytes())
}

#[test]
fn only_an_agent_with_lssame_gets_a_gen_and_a_restarted_one_none() {
    let mut rig = Rig::new("lsr-gen");
    // An older agent decodes the whole argument as the folder's base64.
    rig.h.note_agent_ready("GUEST v3-share2 putdata", rig.now);
    assert_eq!(lsr(&mut rig), format!("LSR {}\n", root()));
    rig.run(6);
    assert_eq!(lsr(&mut rig), format!("LSR {}\n", root()));
    assert_eq!(rig.guest.same_replies, 0);

    rig.h.note_agent_ready(LSSAME_READY, rig.now);
    assert_eq!(lsr(&mut rig), format!("LSR {} 0\n", root()));
    rig.run(6);
    let gen = rig.guest.last_listing.as_ref().unwrap().1.clone();
    assert_eq!(lsr(&mut rig), format!("LSR {} {gen}\n", root()));
    assert!(rig.guest.same_replies > 0, "an unchanged folder was listed whole");
    // A new agent process (its READY) knows no gen.
    rig.h.note_agent_ready(LSSAME_READY, rig.now);
    assert_eq!(lsr(&mut rig), format!("LSR {} 0\n", root()));
}

#[test]
fn a_guest_edit_after_lssame_replies_reaches_the_mac_with_the_next_whole_listing() {
    let mut rig = Rig::new("lssame-edit");
    rig.h.note_agent_ready(LSSAME_READY, rig.now);
    std::fs::write(rig.dir.join("a.txt"), b"one").unwrap();
    rig.guest.write("b.txt", b"from windows");
    rig.run(12);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"one"[..]));
    assert_eq!(rig.host("b.txt").as_deref(), Some(&b"from windows"[..]));
    let same = rig.guest.same_replies;
    rig.assert_settled();
    assert!(rig.guest.same_replies > same, "an unchanged folder was listed whole");
    rig.guest.write("a.txt", b"windows edit");
    rig.run(4);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"windows edit"[..]));
    rig.assert_settled();
}

#[test]
fn lssame_applies_the_kept_listing_again_so_a_failed_write_is_fetched_again() {
    // As the same line sent again did: an unchanged listing plans the GET whose write failed.
    use std::os::unix::fs::PermissionsExt;
    let mode = |rig: &Rig, mode| std::fs::set_permissions(&rig.dir, std::fs::Permissions::from_mode(mode)).unwrap();
    let mut rig = Rig::new("lssame-retry");
    rig.h.note_agent_ready(LSSAME_READY, rig.now);
    mode(&rig, 0o555);
    rig.guest.write("a.txt", b"guest");
    rig.run(12);
    let (gets, same) = (rig.guest.gets.len(), rig.guest.same_replies);
    assert!(gets >= 1);
    rig.run(12);
    assert!(rig.guest.same_replies > same, "the folder was listed whole");
    assert!(rig.guest.gets.len() > gets, "a failed write was not fetched again");
    mode(&rig, 0o755);
    rig.run(12);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"guest"[..]));
    rig.assert_settled();
}

#[test]
fn only_an_applied_listing_is_kept() {
    let mut rig = Rig::new("lssame-kept");
    rig.h.note_agent_ready(LSSAME_READY, rig.now);
    let reply = |rig: &mut Rig, line: &str| {
        rig.h.in_flight = Some((ServiceReq::ShareLs, rig.now));
        rig.h.handle_service_reply(line, None, None, rig.now);
        assert!(rig.h.in_flight.is_none(), "{line}");
    };
    let listing = |text: &str| base64_encode(text.as_bytes());
    reply(&mut rig, &format!("LSOK {} g1", listing("new.txt|1|0|m1\n")));
    assert_eq!(lsr(&mut rig), format!("LSR {} g1\n", root()));
    assert!(matches!(rig.h.queue.pop_front(), Some(ServiceReq::ShareGet { name }) if name == "new.txt"));
    // Applied again: the same GET, as the same line would have planned.
    reply(&mut rig, "LSSAME g1");
    assert!(matches!(rig.h.queue.pop_front(), Some(ServiceReq::ShareGet { name }) if name == "new.txt"));
    assert_eq!(lsr(&mut rig), format!("LSR {} g1\n", root()));
    // An LSSAME for another listing applies nothing, and the next LSR asks for a whole one.
    reply(&mut rig, "LSSAME g2");
    assert!(rig.h.queue.is_empty());
    assert_eq!(lsr(&mut rig), format!("LSR {} 0\n", root()));
    // Neither a refused or failed listing nor an unusable gen is kept.
    let too_many: String = (0..=share_sync::MAX_LISTING_ENTRIES).map(|i| format!("d{i}|0|1|m\n")).collect();
    for line in [
        format!("LSOK {} g3", listing("new.txt|1|0|m1\n")),
        format!("LSOK {} g4", listing(&too_many)),
        format!("LSOK {} g5", listing("bad row\n")),
        "ERR LSR Access to the path is denied.".to_string(),
        format!("LSOK {} bad/gen", listing("new.txt|1|0|m1\n")),
    ] {
        reply(&mut rig, &format!("LSOK {} g6", listing("new.txt|1|0|m1\n")));
        reply(&mut rig, &line);
        let kept = if line.ends_with(" g3") { "g3" } else { "0" };
        assert_eq!(lsr(&mut rig), format!("LSR {} {kept}\n", root()), "after {}", &line[..line.len().min(40)]);
    }
}
