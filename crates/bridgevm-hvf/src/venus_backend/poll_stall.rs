//! Bounded fence-poll stall detection.
//!
//! On macOS there is no renderer sync thread (no eventfd), so
//! `poll_fences` is the only thing that retires renderer fences and writes
//! the shmem FEEDBACK slots Mesa spins on. That poll only advances when a
//! vCPU exit drives the per-exit drain. A guest blocked on its first Venus
//! ring round-trip (`vkEnumerateInstanceVersion` inside `vkCreateInstance`)
//! is therefore ambiguous from the host side:
//!
//!   * the host stopped polling  -> no exits, nothing retires; or
//!   * the host polled normally  -> the renderer never retired the fence.
//!
//! Those two have opposite fixes, so this records which one happened.
//!
//! Deliberately passive: it observes poll calls and never changes poll
//! cadence, fence semantics, or submit acceptance.
//!
//! It also does NOT route through `trace_sample()`, which keeps only the
//! first 64 events of a kind. A stall is interesting precisely because it
//! happens late, so sampling would discard the evidence -- that sampler
//! already caused one incorrect "fences stopped" diagnosis.

use std::time::Duration;

/// Emitted at most `MAX_RECORDS` times per process.
pub(crate) const MAX_STALL_RECORDS: u32 = 16;

/// A gap this long between consecutive polls counts as a stall.
pub(crate) const DEFAULT_STALL_THRESHOLD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollStallRecord {
    /// Wall-clock gap between the previous poll and this one.
    pub(crate) gap: Duration,
    /// Polls completed before this stall was observed.
    pub(crate) polls_before: u64,
    /// Live contexts at stall time.
    pub(crate) contexts: usize,
    /// Fences the host still expects to retire.
    pub(crate) outstanding_fences: usize,
}

/// Tracks inter-poll gaps and yields a bounded number of stall records.
#[derive(Debug, Default)]
pub(crate) struct PollStallDetector {
    polls: u64,
    records_emitted: u32,
}

impl PollStallDetector {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record one poll. `since_last` is the gap since the previous poll, or
    /// `None` for the very first poll (which has no gap to judge).
    ///
    /// Returns a record only when the gap crosses `threshold` and the record
    /// budget is not exhausted, so a persistently stalled run cannot spam.
    pub(crate) fn observe_poll(
        &mut self,
        since_last: Option<Duration>,
        threshold: Duration,
        contexts: usize,
        outstanding_fences: usize,
    ) -> Option<PollStallRecord> {
        let polls_before = self.polls;
        self.polls = self.polls.saturating_add(1);
        let gap = since_last?;
        if gap < threshold || self.records_emitted >= MAX_STALL_RECORDS {
            return None;
        }
        self.records_emitted = self.records_emitted.saturating_add(1);
        Some(PollStallRecord {
            gap,
            polls_before,
            contexts,
            outstanding_fences,
        })
    }
}

impl PollStallRecord {
    /// One-line, grep-able rendering. `polls_before=0` means the host never
    /// polled before this gap (host-side starvation); a large `polls_before`
    /// with outstanding fences means the host was polling and the renderer
    /// did not retire (renderer-side stall).
    pub(crate) fn format(&self, label: &str) -> String {
        format!(
            "{label}: fence-poll stall gap_ms={} polls_before={} contexts={} outstanding_fences={} \
             suspect={}",
            self.gap.as_millis(),
            self.polls_before,
            self.contexts,
            self.outstanding_fences,
            if self.polls_before == 0 {
                "host-never-polled"
            } else if self.outstanding_fences == 0 {
                "no-outstanding-fence"
            } else {
                "renderer-did-not-retire"
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_poll_has_no_gap_to_judge() {
        let mut detector = PollStallDetector::new();
        assert_eq!(
            detector.observe_poll(None, DEFAULT_STALL_THRESHOLD, 1, 1),
            None
        );
        assert_eq!(detector.polls, 1);
    }

    #[test]
    fn gap_below_threshold_is_not_a_stall() {
        let mut detector = PollStallDetector::new();
        detector.observe_poll(None, DEFAULT_STALL_THRESHOLD, 1, 0);
        assert_eq!(
            detector.observe_poll(
                Some(Duration::from_millis(4999)),
                DEFAULT_STALL_THRESHOLD,
                1,
                1
            ),
            None
        );
    }

    #[test]
    fn gap_at_or_past_threshold_is_a_stall() {
        let mut detector = PollStallDetector::new();
        detector.observe_poll(None, DEFAULT_STALL_THRESHOLD, 2, 0);
        let record = detector
            .observe_poll(Some(Duration::from_secs(5)), DEFAULT_STALL_THRESHOLD, 2, 3)
            .expect("a 5s gap at a 5s threshold is a stall");
        assert_eq!(record.gap, Duration::from_secs(5));
        assert_eq!(record.polls_before, 1);
        assert_eq!(record.contexts, 2);
        assert_eq!(record.outstanding_fences, 3);
    }

    #[test]
    fn records_are_bounded() {
        let mut detector = PollStallDetector::new();
        for _ in 0..(MAX_STALL_RECORDS + 50) {
            detector.observe_poll(Some(Duration::from_secs(60)), DEFAULT_STALL_THRESHOLD, 1, 1);
        }
        assert_eq!(detector.records_emitted, MAX_STALL_RECORDS);
    }

    #[test]
    fn suspect_distinguishes_host_starvation_from_renderer_stall() {
        let starved = PollStallRecord {
            gap: Duration::from_secs(9),
            polls_before: 0,
            contexts: 1,
            outstanding_fences: 1,
        };
        assert!(starved
            .format("venus")
            .contains("suspect=host-never-polled"));

        let renderer = PollStallRecord {
            gap: Duration::from_secs(9),
            polls_before: 4321,
            contexts: 2,
            outstanding_fences: 7,
        };
        let line = renderer.format("venus");
        assert!(line.contains("suspect=renderer-did-not-retire"));
        assert!(line.contains("gap_ms=9000"));
        assert!(line.contains("outstanding_fences=7"));
    }
}
