//! CLIPCAPS PUSH: an agent that writes CLIPCHANGED is polled on each notice
//! and otherwise every 10 s; an older agent keeps the 1 s poll.

use super::*;

fn caps(rig: &mut ClipRig, reply: &str) {
    rig.h.queue.push_back(ServiceReq::ClipCaps);
    assert_eq!(rig.send(), "CLIPCAPS PUSH\n");
    rig.h.handle_service_reply(reply, None, None, rig.now);
    assert!(rig.h.in_flight.is_none());
}

/// Caps answered, then the first poll adopted the guest's text.
fn negotiated(reply: &str) -> ClipRig {
    let mut rig = ClipRig::new("guest-1");
    caps(&mut rig, reply);
    rig.settle();
    assert_eq!(rig.mac_sets(), vec!["guest-1".to_owned()]);
    rig
}

fn polls_queued(rig: &ClipRig) -> usize {
    rig.h
        .queue
        .iter()
        .filter(|req| matches!(req, ServiceReq::ClipPoll))
        .count()
}

#[test]
fn a_push_agent_is_polled_on_each_notice_and_every_10_s() {
    let mut rig = negotiated("CLIPCAPS CLIPPUSH");
    assert!(rig.h.clip_push);
    rig.guest = "guest-2".into();
    for second in 1..=8 {
        rig.enqueue();
        assert_eq!(polls_queued(&rig), 0, "polled {second} s later without a notice");
    }
    rig.h.handle_service_reply("CLIPCHANGED", None, None, rig.now);
    rig.h.handle_service_reply("CLIPCHANGED", None, None, rig.now);
    assert_eq!(polls_queued(&rig), 1, "a notice queues one poll");
    let line = rig.send();
    rig.answer(&line);
    assert_eq!(rig.mac_sets(), vec!["guest-2".to_owned()]);
    for _ in 1..=9 {
        rig.enqueue();
    }
    assert_eq!(polls_queued(&rig), 0);
    rig.enqueue();
    assert_eq!(polls_queued(&rig), 1, "the 10 s safety poll");
}

#[test]
fn an_agent_without_push_keeps_the_1_s_poll() {
    let mut rig = negotiated(&format!(
        "OUT 255 {}",
        base64_encode(b"unknown token: CLIPCAPS")
    ));
    assert!(!rig.h.clip_push);
    rig.enqueue();
    assert_eq!(polls_queued(&rig), 1);
}

#[test]
fn a_notice_during_a_command_leaves_the_command_waiting_for_its_reply() {
    let mut rig = negotiated("CLIPCAPS CLIPPUSH");
    rig.h.queue.push_back(ServiceReq::Ctl("whoami".into()));
    rig.send();
    rig.h.handle_service_reply("CLIPCHANGED", None, None, rig.now);
    assert!(matches!(rig.h.in_flight, Some((ServiceReq::Ctl(_), _))));
    assert_eq!(polls_queued(&rig), 1);
    let out = format!("OUT 0 {}", base64_encode(b"bv\\user\r\n"));
    rig.h.handle_service_reply(&out, None, None, rig.now);
    assert!(rig.h.in_flight.is_none());
}

#[test]
fn push_comes_with_images_and_ends_with_an_agent_restart() {
    let mut rig = ClipRig::new("guest-1");
    let dir = base64_encode(b"C:\\ProgramData\\BridgeVM\\clip");
    caps(&mut rig, &format!("CLIPCAPS IMAGE_PNG {dir} CLIPPUSH"));
    assert!(rig.h.clip_images.is_some() && rig.h.clip_push);
    rig.h
        .handle_service_reply("READY BRIDGEVM v3-share2 agent=new", None, None, rig.now);
    assert!(!rig.h.clip_push, "the new agent sends no notices before CLIPCAPS");
    assert!(matches!(rig.h.queue.front(), Some(ServiceReq::ClipCaps)));
}
