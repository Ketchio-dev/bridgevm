//! Worker/channel lifecycle controls with an in-memory clipboard only.

use super::*;

pub(super) struct MemoryPasteboard {
    pub(super) text: String,
    pub(super) count: i64,
    pub(super) writes: Vec<String>,
}

impl MemoryPasteboard {
    pub(super) fn new(text: &str) -> Self {
        Self {
            text: text.into(),
            count: 1,
            writes: Vec::new(),
        }
    }
}

impl PasteboardBackend for MemoryPasteboard {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        Some(self.text.clone())
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.text = text.into();
        self.count += 1;
        self.writes.push(text.into());
        true
    }
}

#[test]
fn initial_host_content_wins_over_guest_write_queued_before_first_poll() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("host-startup");
    bridge.set("guest-before-startup".into());
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.text, "host-startup");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("host-startup"));
    assert!(mac.writes.is_empty());
}

#[test]
fn closed_service_discards_pending_writes_and_fresh_bridge_still_works() {
    let (old, mut old_worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("");
    assert!(old_worker.poll(&mut mac));
    old.set("old-session".into());
    drop(old);
    assert!(!old_worker.apply_pending(&mut mac));
    assert!(
        mac.writes.is_empty(),
        "closed service's queued write was applied"
    );
    let (fresh, mut worker) = HostPasteboard::test_pair();
    fresh.set("fresh-session".into());
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["fresh-session"]);
}

#[test]
fn empty_startup_accepts_guest_text_without_echo() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("");
    bridge.set("guest".into());
    assert!(worker.apply_pending(&mut mac));
    assert!(worker.poll(&mut mac));
    assert_eq!(mac.text, "guest");
    assert_eq!(bridge.try_changed(), None);
}

#[test]
fn newest_observed_copy_survives_stale_set_then_accepts_consecutive_guest_updates() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("baseline");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("baseline"));
    bridge.set("stale-guest".into());
    for text in ["host-1", "host-2"] {
        mac.text = text.into();
        mac.count += 1;
        assert!(worker.poll(&mut mac));
    }
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.text, "host-2");
    assert!(mac.writes.is_empty());
    assert_eq!(bridge.try_changed().as_deref(), Some("host-1"));
    assert_eq!(bridge.try_changed().as_deref(), Some("host-2"));
    bridge.set("guest-1".into());
    bridge.set("guest-2".into());
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.writes, ["guest-1", "guest-2"]);
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed(), None);
}

#[test]
fn unobserved_host_copy_still_refuses_guest_set_and_reports_copy() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("baseline");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("baseline"));
    bridge.set("stale-guest".into());
    mac.text = "unobserved-host".into();
    mac.count += 1;
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(mac.text, "unobserved-host");
    assert!(worker.poll(&mut mac));
    assert_eq!(bridge.try_changed().as_deref(), Some("unobserved-host"));
}

#[test]
fn exhausted_revision_stops_without_applying_old_writes() {
    let (bridge, mut worker) = HostPasteboard::test_pair();
    let mut mac = MemoryPasteboard::new("host");
    worker.revision = Some(u64::MAX);
    bridge.observed.set(u64::MAX);
    bridge.set("stale-guest".into());
    assert!(!worker.poll(&mut mac));
    assert!(!worker.apply_pending(&mut mac));
    assert_eq!(mac.text, "host");
    assert!(mac.writes.is_empty());
    assert_eq!(bridge.try_changed(), None);
}
