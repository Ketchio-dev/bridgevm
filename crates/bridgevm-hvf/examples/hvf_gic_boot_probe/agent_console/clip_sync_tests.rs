//! Clipboard auto-sync through the resident service loop: a host copy the
//! guest never confirmed must not let the guest's older text revert the Mac.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::sync::mpsc::{Receiver, Sender};

/// Service loop wired to a channel-only pasteboard and a modelled guest
/// clipboard that answers CLIPGET/CLIPSET the way `bvagent.ps1` does.
struct ClipRig {
    h: AgentConsoleHarness,
    mac_copy: Sender<String>,
    mac_sets: Receiver<String>,
    guest: String,
    fail_set: bool,
    now: Instant,
}

impl ClipRig {
    fn new(guest: &str) -> Self {
        let (pasteboard, mac_copy, mac_sets) = HostPasteboard::test_pair();
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.clipsync = true;
        h.pasteboard = Some(pasteboard);
        let now = Instant::now();
        let guest = guest.to_owned();
        Self {
            h,
            mac_copy,
            mac_sets,
            guest,
            fail_set: false,
            now,
        }
    }
    /// Steady state: the guest's text was adopted onto the Mac by a poll.
    fn synced(guest: &str) -> Self {
        let mut rig = Self::new(guest);
        rig.settle();
        assert_eq!(rig.mac_sets(), vec![guest.to_owned()]);
        rig
    }
    fn copy_on_mac(&self, text: &str) {
        self.mac_copy.send(text.to_owned()).unwrap();
    }
    /// One service_enqueue pass, one clip interval later.
    fn enqueue(&mut self) {
        self.now += Duration::from_secs(1);
        self.h.service_enqueue(self.now);
    }
    /// Put the next queued request on the wire, as service_pump does.
    fn send(&mut self) -> String {
        let req = self.h.queue.pop_front().expect("queued request");
        let line = self.h.service_req_line(&req, self.now).unwrap().to_owned();
        self.h.in_flight = Some((req, self.now));
        line
    }
    fn answer(&mut self, line: &str) {
        let reply = if line == "CLIPGET\n" {
            format!("CLIP {}", base64_encode(self.guest.as_bytes()))
        } else if let Some(b64) = line.trim_end().strip_prefix("CLIPSET ") {
            if self.fail_set {
                "ERR CLIPSET Requested Clipboard operation did not succeed.".to_owned()
            } else {
                self.guest = String::from_utf8(base64_decode(b64).unwrap()).unwrap();
                "OK CLIPSET".to_owned()
            }
        } else {
            panic!("unexpected request {line:?}");
        };
        self.h.handle_service_reply(&reply, None, None, self.now);
    }
    /// Enqueue, then answer every queued request in lockstep.
    fn settle(&mut self) {
        self.enqueue();
        while !self.h.queue.is_empty() {
            let line = self.send();
            self.answer(&line);
        }
        assert!(self.h.in_flight.is_none());
    }
    fn mac_sets(&self) -> Vec<String> {
        self.mac_sets.try_iter().collect()
    }
}

#[test]
fn oversized_mac_copy_is_not_reverted_by_older_guest_text() {
    let mut rig = ClipRig::synced("old");
    let big = "x".repeat(rig.h.clip_max_bytes + 1);
    rig.copy_on_mac(&big);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "older guest text written over the Mac copy"
    );
    assert_eq!(rig.guest, "old");
    // A genuine later guest copy still syncs.
    rig.guest = "guest-new".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), vec!["guest-new".to_owned()]);
}

#[test]
fn oversized_mac_copy_at_startup_is_not_reverted_by_guest_text() {
    let mut rig = ClipRig::new("guest-old");
    rig.copy_on_mac(&"y".repeat(rig.h.clip_max_bytes + 1));
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "guest text written over the Mac copy"
    );
}

#[test]
fn failed_clipset_does_not_revert_mac_copy() {
    let mut rig = ClipRig::synced("old");
    rig.fail_set = true;
    rig.copy_on_mac("mac-new");
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "older guest text written over the Mac copy"
    );
    assert_eq!(rig.guest, "old");
}

#[test]
fn clip_push_dropped_by_agent_restart_does_not_revert_mac_copy() {
    let mut rig = ClipRig::synced("old");
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    assert!(rig.send().starts_with("CLIPSET "));
    rig.h
        .handle_service_reply("READY GUEST", None, None, rig.now);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "older guest text written over the Mac copy"
    );
}

#[test]
fn clipget_in_flight_ahead_of_push_does_not_flip_mac_copy() {
    let mut rig = ClipRig::synced("old");
    rig.enqueue();
    let poll = rig.send();
    assert_eq!(poll, "CLIPGET\n");
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    rig.answer(&poll);
    while !rig.h.queue.is_empty() {
        let line = rig.send();
        rig.answer(&line);
    }
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "Mac copy flipped to older guest text"
    );
    assert_eq!(rig.guest, "mac-new");
}

#[test]
fn confirmed_push_does_not_bounce_and_later_guest_copy_syncs() {
    let mut rig = ClipRig::synced("old");
    rig.copy_on_mac("mac-new\nline");
    rig.settle();
    assert_eq!(rig.guest, "mac-new\r\nline");
    rig.settle();
    assert!(rig.mac_sets().is_empty());
    rig.guest = "guest-new".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), vec!["guest-new".to_owned()]);
}

#[test]
fn mac_copy_back_to_synced_text_while_push_in_flight_still_lands() {
    let mut rig = ClipRig::synced("old");
    rig.copy_on_mac("mac-1");
    rig.enqueue();
    let push = rig.send();
    rig.copy_on_mac("old");
    rig.enqueue();
    rig.answer(&push);
    rig.settle();
    rig.settle();
    assert_eq!(rig.guest, "old");
    assert!(rig.mac_sets().is_empty());
}

#[test]
fn clipget_overtaken_by_push_does_not_adopt_concurrent_guest_text() {
    let mut rig = ClipRig::synced("old");
    rig.enqueue();
    let poll = rig.send();
    rig.guest = "guest-y".into();
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    rig.answer(&poll);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "Mac copy replaced while its push was pending"
    );
    assert_eq!(rig.guest, "mac-new");
}

#[test]
fn restart_dropped_push_before_guest_text_known_is_not_reverted() {
    let mut rig = ClipRig::new("guest-old");
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    assert!(rig.send().starts_with("CLIPSET "));
    rig.h
        .handle_service_reply("READY GUEST", None, None, rig.now);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "guest text written over the Mac copy"
    );
}

#[test]
fn baseline_advances_to_host_copy_only_on_clipset_ok() {
    let mut rig = ClipRig::synced("old");
    rig.copy_on_mac("mac-new");
    rig.enqueue();
    let push = rig.send();
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("old"));
    rig.answer(&push);
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("mac-new"));
}

#[test]
fn mac_copy_of_stale_baseline_is_pushed_while_guest_text_unknown() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "guest-y".into();
    rig.copy_on_mac(&"z".repeat(rig.h.clip_max_bytes + 1));
    rig.enqueue();
    rig.copy_on_mac("old");
    rig.settle();
    rig.settle();
    assert_eq!(rig.guest, "old");
    assert!(rig.mac_sets().is_empty());
}
