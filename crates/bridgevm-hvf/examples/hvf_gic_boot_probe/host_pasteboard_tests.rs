//! NSPasteboard poller state machine against a scripted backend (no real
//! pasteboard is touched).

use super::{PasteboardBackend, Poller};

struct MockBackend {
    count: i64,
    text: Option<String>,
    set_success: bool,
}

impl MockBackend {
    fn new(count: i64, text: Option<&str>) -> Self {
        Self {
            count,
            text: text.map(str::to_owned),
            set_success: true,
        }
    }

    fn external_change(&mut self, count: i64, text: Option<&str>) {
        self.count = count;
        self.text = text.map(str::to_owned);
    }
}

impl PasteboardBackend for MockBackend {
    fn change_count(&mut self) -> i64 {
        self.count
    }

    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_string(&mut self, text: &str) -> bool {
        if !self.set_success {
            return false;
        }
        self.count += 1;
        self.text = Some(text.to_owned());
        true
    }
}

#[test]
fn startup_content_emitted_exactly_once_on_first_tick() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(7, Some("startup"));

    assert_eq!(poller.on_tick(&mut backend), Some("startup".to_owned()));
    assert_eq!(poller.on_tick(&mut backend), None);
}

#[test]
fn external_change_emits_once_and_unchanged_count_emits_nothing() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some("old"));

    assert_eq!(poller.on_tick(&mut backend), Some("old".to_owned()));
    backend.external_change(2, Some("new"));

    assert_eq!(poller.on_tick(&mut backend), Some("new".to_owned()));
    assert_eq!(poller.on_tick(&mut backend), None);
}

#[test]
fn apply_set_does_not_echo_on_following_tick() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(0, None);

    poller.apply_set(&mut backend, "guest");

    assert_eq!(poller.on_tick(&mut backend), None);
}

#[test]
fn external_same_text_after_apply_set_does_not_reemit() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(0, None);

    poller.apply_set(&mut backend, "same");
    backend.external_change(backend.count + 1, Some("same"));

    assert_eq!(poller.on_tick(&mut backend), None);
}

#[test]
fn empty_string_content_is_never_emitted() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some(""));

    assert_eq!(poller.on_tick(&mut backend), None);
    backend.external_change(2, Some(""));
    assert_eq!(poller.on_tick(&mut backend), None);
}

#[test]
fn failed_set_string_leaves_poller_state_unchanged() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some("host"));

    assert_eq!(poller.on_tick(&mut backend), Some("host".to_owned()));
    backend.set_success = false;
    poller.apply_set(&mut backend, "guest");
    // The failed write must not be recorded as seen: a later real "guest"
    // copy on the Mac is still reported.
    backend.external_change(2, Some("guest"));

    assert_eq!(poller.on_tick(&mut backend), Some("guest".to_owned()));
}

/// `run()` drains queued set() requests before polling. A Mac copy made after
/// the last poll is newer than the queued guest text: it must survive and be
/// reported by the next poll, not be overwritten and lost.
#[test]
fn queued_set_does_not_overwrite_newer_host_copy() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some("synced"));
    assert_eq!(poller.on_tick(&mut backend), Some("synced".to_owned()));

    backend.external_change(2, Some("host-new"));
    poller.apply_set(&mut backend, "guest-old");

    assert_eq!(backend.text.as_deref(), Some("host-new"));
    assert_eq!(poller.on_tick(&mut backend), Some("host-new".to_owned()));
}

/// A skipped guest write must still let the Mac text win, even when the
/// changeCount moved but the text equals what was last reported.
#[test]
fn skipped_set_re_reports_unchanged_host_text() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some("host"));
    assert_eq!(poller.on_tick(&mut backend), Some("host".to_owned()));

    backend.external_change(2, Some("host"));
    poller.apply_set(&mut backend, "guest");

    assert_eq!(backend.text.as_deref(), Some("host"));
    assert_eq!(poller.on_tick(&mut backend), Some("host".to_owned()));
}

#[test]
fn set_after_observed_count_still_applies_without_echo() {
    let mut poller = Poller::new();
    let mut backend = MockBackend::new(1, Some("host"));
    assert_eq!(poller.on_tick(&mut backend), Some("host".to_owned()));

    poller.apply_set(&mut backend, "guest");

    assert_eq!(backend.text.as_deref(), Some("guest"));
    assert_eq!(poller.on_tick(&mut backend), None);
    backend.set_success = false;
    poller.apply_set(&mut backend, "lost");
    backend.external_change(backend.count + 1, Some("host-2"));
    assert_eq!(poller.on_tick(&mut backend), Some("host-2".to_owned()));
}
