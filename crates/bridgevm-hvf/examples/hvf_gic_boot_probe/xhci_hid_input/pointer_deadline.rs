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
            .find(|checkpoint| !checkpoint.emitted)
            .and_then(|checkpoint| fired_at.checked_add(checkpoint.delay))
    });
    [report_deadline, checkpoint_deadline]
        .into_iter()
        .flatten()
        .filter(|deadline| *deadline > now)
        .min()
}
