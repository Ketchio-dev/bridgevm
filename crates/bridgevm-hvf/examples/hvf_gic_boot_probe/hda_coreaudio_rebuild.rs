//! When the supervisor replaces the output queue, without any CoreAudio call.
//!
//! The queue names no device, so it plays on whatever was the Mac's default
//! output when it was created; nothing moved it later. Headphones connected
//! afterwards stayed silent, and a removed device left sound off until the VM
//! restarted. The supervisor (hda_coreaudio_output.rs) therefore builds a new
//! queue on the current default output, keeping the ring, and retires the old
//! one when:
//!
//! - the default output changed (a kAudioHardwarePropertyDefaultOutputDevice
//!   listener, or the test hook BRIDGEVM_HDA_COREAUDIO_TEST_REBUILD_MS, which
//!   reports a change every so many milliseconds while the queue plays);
//! - starting the paused queue failed;
//! - a callback could not enqueue its buffer while playing, so the queue lost
//!   one of its three buffers;
//! - the queue stopped running while it should play. A paused queue still
//!   reports running, and a device may report its start late, so only a queue
//!   started at least `START_GRACE` earlier is asked.

use std::time::{Duration, Instant};

pub(super) const START_GRACE: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Rebuild {
    DefaultOutputChanged,
    StartFailed,
    EnqueueFailed,
    StoppedRunning,
}

impl Rebuild {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::DefaultOutputChanged => "the Mac's default output changed",
            Self::StartFailed => "the paused output did not start",
            Self::EnqueueFailed => "a buffer could not be queued",
            Self::StoppedRunning => "the output stopped running",
        }
    }
}

/// What the supervisor learned since its last step.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Signals {
    pub(super) default_output_changed: bool,
    pub(super) start_failed: bool,
    pub(super) enqueue_failed: bool,
}

/// The queue's run state as the supervisor last set it.
pub(super) struct Watch {
    started_at: Option<Instant>,
}

impl Watch {
    /// A queue started at `now`.
    pub(super) fn started(now: Instant) -> Self {
        Self {
            started_at: Some(now),
        }
    }

    pub(super) fn note_started(&mut self, now: Instant) {
        self.started_at = Some(now);
    }

    pub(super) fn note_paused(&mut self) {
        self.started_at = None;
    }

    /// Whether to replace the queue at `now`. `is_running` is asked only of a
    /// queue that should play and started at least `START_GRACE` earlier.
    pub(super) fn rebuild(
        &self,
        signals: Signals,
        now: Instant,
        is_running: impl FnOnce() -> bool,
    ) -> Option<Rebuild> {
        if signals.default_output_changed {
            return Some(Rebuild::DefaultOutputChanged);
        }
        if signals.start_failed {
            return Some(Rebuild::StartFailed);
        }
        if signals.enqueue_failed {
            return Some(Rebuild::EnqueueFailed);
        }
        let settled = self
            .started_at
            .is_some_and(|at| now.saturating_duration_since(at) >= START_GRACE);
        (settled && !is_running()).then_some(Rebuild::StoppedRunning)
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_rebuild_tests.rs"]
mod tests;
