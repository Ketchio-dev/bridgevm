use std::time::{Duration, Instant};

pub(super) const CADENCE: Duration = Duration::from_millis(16);
pub(super) const BURST: Duration = Duration::from_millis(500);

pub(super) fn wake_due(fired: bool, now: Instant, schedule: Option<(Instant, Instant)>) -> bool {
    !fired && schedule.is_some_and(|(next, until)| now >= next && now <= until)
}

#[test]
fn wake_burst_is_bounded_and_non_overlapping() {
    let now = Instant::now();
    assert!(!wake_due(false, now, None));
    assert!(wake_due(false, now, Some((now, now + BURST))));
    assert!(!wake_due(true, now, Some((now, now + BURST))));
    assert!(!wake_due(false, now + BURST + Duration::from_millis(2),
        Some((now, now + BURST))));
}
