//! Empty and unavailable text still represent host clipboard ownership changes.
//! Only the memory backend is touched; the actual service and worker are used.

use super::*;
use crate::agent_console::host_pasteboard::PasteboardBackend;

struct MemoryClipboard {
    text: Option<String>,
    count: i64,
    writes: Vec<String>,
}

impl MemoryClipboard {
    fn copy(&mut self, text: Option<&str>) {
        self.text = text.map(str::to_owned);
        self.count += 1;
    }
}

impl PasteboardBackend for MemoryClipboard {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.copy(Some(text));
        self.writes.push(text.to_owned());
        true
    }
}

fn ready() -> (ClipRig, MemoryClipboard) {
    let mut rig = ClipRig::new("initial-host");
    let mut mac = MemoryClipboard {
        text: Some("initial-host".into()),
        count: 1,
        writes: Vec::new(),
    };
    assert!(rig.pasteboard.poll(&mut mac));
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert!(mac.writes.is_empty());
    (rig, mac)
}

fn queued_write_is_refused(text: Option<&str>, observe_first: bool) {
    let (mut rig, mut mac) = ready();
    rig.guest = "queued-old-guest".into();
    rig.settle();
    mac.copy(text);
    if observe_first {
        assert!(rig.pasteboard.poll(&mut mac));
    }
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text.as_deref(), text, "older guest write erased host ownership");
    assert!(mac.writes.is_empty(), "not even an intermediate stale write is allowed");
    assert!(rig.pasteboard.poll(&mut mac));
    for _ in 0..3 {
        rig.settle();
        assert!(rig.pasteboard.apply_pending(&mut mac));
        assert!(rig.pasteboard.poll(&mut mac));
    }
    assert_eq!(mac.text.as_deref(), text);
    assert_eq!(rig.guest, "queued-old-guest", "no empty or non-text payload is sent");
    assert!(mac.writes.is_empty());
    rig.guest = "genuine-later-guest".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["genuine-later-guest"]);
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
    rig.settle();
    assert!(rig.mac_sets().is_empty(), "accepted guest text must not echo");
}

#[test]
fn observed_nontext_copy_refuses_older_queued_guest_text() {
    queued_write_is_refused(None, true);
}

#[test]
fn observed_empty_copy_refuses_older_queued_guest_text() {
    queued_write_is_refused(Some(""), true);
}

#[test]
fn unobserved_nontext_copy_retains_existing_write_admission() {
    queued_write_is_refused(None, false);
}

#[test]
fn unobserved_empty_copy_retains_existing_write_admission() {
    queued_write_is_refused(Some(""), false);
}

fn returning_host_text_is_reported(text: Option<&str>) {
    let (mut rig, mut mac) = ready();
    mac.copy(text);
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
    mac.copy(Some("initial-host"));
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(
        rig.h.pasteboard.as_ref().unwrap().try_changed().as_deref(),
        Some("initial-host"),
        "text returning after another clipboard type is a new host report"
    );
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
    assert!(mac.writes.is_empty());
}

#[test]
fn host_text_returning_after_nontext_is_not_suppressed() {
    returning_host_text_is_reported(None);
}

#[test]
fn host_text_returning_after_empty_is_not_suppressed() {
    returning_host_text_is_reported(Some(""));
}

fn stale_then_fresh_fifo(text: Option<&str>) {
    let (mut rig, mut mac) = ready();
    rig.guest = "old-guest".into();
    rig.settle();
    mac.copy(text);
    assert!(rig.pasteboard.poll(&mut mac));
    rig.guest = "later-guest".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["later-guest"], "only the fresh FIFO write is eligible");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
}

#[test]
fn nontext_copy_invalidates_only_older_fifo_writes() {
    stale_then_fresh_fifo(None);
}

#[test]
fn empty_copy_invalidates_only_older_fifo_writes() {
    stale_then_fresh_fifo(Some(""));
}
