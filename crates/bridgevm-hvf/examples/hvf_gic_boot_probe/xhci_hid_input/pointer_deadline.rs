//! Shared ordering for pointer pacing and diagnostic checkpoint wakeups.

use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct PointerRamfbDelayCheckpoint {
    pub(super) label: String,
    pub(super) delay: Duration,
    pub(super) emitted: bool,
}

pub(super) fn next_deadline(
    now: Instant,
    report_deadline: Option<Instant>,
    fired_at: Option<Instant>,
    checkpoints: &[PointerRamfbDelayCheckpoint],
) -> Option<Instant> {
    let checkpoint_deadline = fired_at.and_then(|fired_at| {
        checkpoints
            .iter()
            .filter(|checkpoint| !checkpoint.emitted)
            .filter_map(|checkpoint| fired_at.checked_add(checkpoint.delay))
            .filter(|deadline| *deadline > now)
            .min()
    });
    let deadlines = [report_deadline, checkpoint_deadline];
    deadlines.into_iter().flatten().filter(|deadline| *deadline > now).min()
}
