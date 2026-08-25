//! Deduplicate print-only overdue observations for one process-local VM.

use std::sync::Mutex;
use std::time::Instant;

static LAST: Mutex<Option<Instant>> = Mutex::new(None);

pub(super) fn first(deadline: Option<Instant>) -> Option<Instant> {
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match deadline {
        Some(deadline) if *last != Some(deadline) => {
            *last = Some(deadline);
            Some(deadline)
        }
        Some(_) => None,
        None => {
            *last = None;
            None
        }
    }
}

#[cfg(test)]
#[test]
fn one_deadline_is_reported_once_and_none_rearms_it() {
    let deadline = Instant::now();
    first(None);
    assert_eq!(first(Some(deadline)), Some(deadline));
    assert_eq!(first(Some(deadline)), None);
    first(None);
    assert_eq!(first(Some(deadline)), Some(deadline));
    first(None);
    let lateness = super::xhci_pointer_overdue_trace::lateness_us;
    assert_eq!(lateness(deadline, deadline), None);
    assert_eq!(
        lateness(deadline + std::time::Duration::from_micros(37), deadline),
        Some(37)
    );
}
