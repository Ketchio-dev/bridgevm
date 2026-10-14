//! An external recopy after a queued guest write still owns the Mac clipboard,
//! even if its bytes match the previous host report or guest-origin self-write.

use super::*;
use crate::agent_console::host_pasteboard::PasteboardBackend;

struct MemoryPasteboard {
    text: String,
    count: i64,
    writes: Vec<String>,
}

impl PasteboardBackend for MemoryPasteboard {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        Some(self.text.clone())
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.writes.push(text.into());
        self.text = text.into();
        self.count += 1;
        true
    }
}

fn recopy_wins_over_queued_guest_write(guest_origin: bool) {
    let mut rig = ClipRig::new("same-host-text");
    let mut mac = MemoryPasteboard {
        text: if guest_origin { "" } else { "same-host-text" }.into(),
        count: 1,
        writes: Vec::new(),
    };
    assert!(rig.pasteboard.poll(&mut mac));
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "same-host-text");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);

    rig.guest = "guest-before-recopy".into();
    rig.enqueue();
    let poll = rig.send();
    assert_eq!(poll, "CLIPGET\n");
    rig.answer(&poll);
    // The service has queued a guest write, but the user copies the existing
    // Mac text again before the worker can apply that queued command.
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    assert!(rig.pasteboard.apply_pending(&mut mac));
    rig.settle();
    rig.settle();
    assert_eq!(
        (mac.text.as_str(), rig.guest.as_str()),
        ("same-host-text", "same-host-text"),
        "new Mac ownership was erased by the older queued guest write"
    );
    assert!(rig.mac_sets().is_empty(), "settled state must not echo");
    rig.guest = "genuine-later-guest".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "genuine-later-guest");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
}

#[test]
fn same_host_recopy_supersedes_queued_guest_write() {
    recopy_wins_over_queued_guest_write(false);
}

#[test]
fn same_guest_origin_recopy_supersedes_queued_guest_write() {
    recopy_wins_over_queued_guest_write(true);
}

fn ready() -> (ClipRig, MemoryPasteboard) {
    let mut rig = ClipRig::new("baseline");
    let mut mac = MemoryPasteboard {
        text: "baseline".into(),
        count: 1,
        writes: Vec::new(),
    };
    assert!(rig.pasteboard.poll(&mut mac));
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    (rig, mac)
}

#[test]
fn no_conflict_recopy_stays_suppressed_and_later_guest_still_applies() {
    let (mut rig, mut mac) = ready();
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
    rig.guest = "later-guest".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "later-guest");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
}

#[test]
fn stale_then_fresh_guest_fifo_keeps_later_guest_without_host_rereport() {
    let (mut rig, mut mac) = ready();
    rig.guest = "stale-guest".into();
    rig.settle();
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    rig.guest = "fresh-guest".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "fresh-guest");
    assert_eq!(mac.writes, ["fresh-guest"], "stale write must never apply");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
    rig.settle();
    assert!(rig.mac_sets().is_empty());
}

#[test]
fn recopy_rereport_then_distinct_host_report_preserve_service_order() {
    let (mut rig, mut mac) = ready();
    rig.guest = "stale-guest".into();
    rig.settle();
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "baseline");
    // The recopy report is queued; a second distinct observation is newer.
    mac.text = "newest-host".into();
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    rig.settle();
    rig.settle();
    assert_eq!(rig.guest, "newest-host");
    assert!(rig.mac_sets().is_empty());
    rig.guest = "guest-after-both-reports".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "guest-after-both-reports");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
}
