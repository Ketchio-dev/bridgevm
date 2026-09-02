//! Typed AudioQueue callback-enqueue failure classification.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::ffi::c_void;

pub(super) unsafe fn dispose_failed_queue<T>(
    queue: *mut c_void,
    context: *mut T,
    dispose: unsafe extern "C" fn(*mut c_void, u8) -> i32,
) {
    let status = unsafe { dispose(queue, 1) };
    if status == 0 {
        unsafe { drop(Box::from_raw(context)) };
    } else {
        eprintln!("AudioQueueDispose cleanup failed with OSStatus {status} ({status:#010x})");
    }
}

const PHASE_ACTIVE: u8 = 0;
const PHASE_STOPPING: u8 = 1;
const INVALID_RUN_STATE: i32 = -66_678;
const QUEUE_INVALIDATED: i32 = -66_671;
const ENQUEUE_DURING_RESET: i32 = -66_632;
const DISPOSAL_PENDING: i32 = -66_685;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallbackFailureReason {
    ActiveEnqueueFailure,
    StoppingInvalidRunState,
    StoppingQueueInvalidated,
    StoppingEnqueueDuringReset,
    StoppingDisposalPending,
    StoppingUnclassified,
}

impl CallbackFailureReason {
    fn classify(stopping: bool, status: i32) -> Self {
        if !stopping {
            return Self::ActiveEnqueueFailure;
        }
        match status {
            INVALID_RUN_STATE => Self::StoppingInvalidRunState,
            QUEUE_INVALIDATED => Self::StoppingQueueInvalidated,
            ENQUEUE_DURING_RESET => Self::StoppingEnqueueDuringReset,
            DISPOSAL_PENDING => Self::StoppingDisposalPending,
            _ => Self::StoppingUnclassified,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ActiveEnqueueFailure => "active-enqueue-failure",
            Self::StoppingInvalidRunState => "stopping-invalid-run-state",
            Self::StoppingQueueInvalidated => "stopping-queue-invalidated",
            Self::StoppingEnqueueDuringReset => "stopping-enqueue-during-reset",
            Self::StoppingDisposalPending => "stopping-disposal-pending",
            Self::StoppingUnclassified => "stopping-unclassified",
        }
    }

    fn expected(self) -> bool {
        matches!(
            self,
            Self::StoppingInvalidRunState
                | Self::StoppingEnqueueDuringReset
                | Self::StoppingDisposalPending
        )
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct CallbackFailureSnapshot {
    pub(super) total: u64,
    pub(super) active: u64,
    pub(super) stopping: u64,
    pub(super) expected_stopping: u64,
    pub(super) unexpected: u64,
    pub(super) invalid_run_state: u64,
    pub(super) queue_invalidated: u64,
    pub(super) enqueue_during_reset: u64,
    pub(super) disposal_pending: u64,
    pub(super) stopping_unclassified: u64,
}

pub(super) struct CallbackFailureCounters {
    phase: AtomicU8,
    total: AtomicU64,
    active: AtomicU64,
    stopping: AtomicU64,
    expected_stopping: AtomicU64,
    unexpected: AtomicU64,
    invalid_run_state: AtomicU64,
    queue_invalidated: AtomicU64,
    enqueue_during_reset: AtomicU64,
    disposal_pending: AtomicU64,
    stopping_unclassified: AtomicU64,
}

impl CallbackFailureCounters {
    pub(super) fn new() -> Self {
        Self {
            phase: AtomicU8::new(PHASE_ACTIVE),
            total: AtomicU64::new(0),
            active: AtomicU64::new(0),
            stopping: AtomicU64::new(0),
            expected_stopping: AtomicU64::new(0),
            unexpected: AtomicU64::new(0),
            invalid_run_state: AtomicU64::new(0),
            queue_invalidated: AtomicU64::new(0),
            enqueue_during_reset: AtomicU64::new(0),
            disposal_pending: AtomicU64::new(0),
            stopping_unclassified: AtomicU64::new(0),
        }
    }

    pub(super) fn begin_stopping(&self) {
        self.phase.store(PHASE_STOPPING, Ordering::Release);
    }

    pub(super) fn record(&self, status: i32) {
        let stopping = self.phase.load(Ordering::Acquire) == PHASE_STOPPING;
        let reason = CallbackFailureReason::classify(stopping, status);
        self.total.fetch_add(1, Ordering::Relaxed);
        if stopping {
            self.stopping.fetch_add(1, Ordering::Relaxed);
        } else {
            self.active.fetch_add(1, Ordering::Relaxed);
        }
        if reason.expected() {
            self.expected_stopping.fetch_add(1, Ordering::Relaxed);
        } else {
            self.unexpected.fetch_add(1, Ordering::Relaxed);
        }
        match reason {
            CallbackFailureReason::StoppingInvalidRunState => &self.invalid_run_state,
            CallbackFailureReason::StoppingQueueInvalidated => &self.queue_invalidated,
            CallbackFailureReason::StoppingEnqueueDuringReset => &self.enqueue_during_reset,
            CallbackFailureReason::StoppingDisposalPending => &self.disposal_pending,
            CallbackFailureReason::StoppingUnclassified => &self.stopping_unclassified,
            CallbackFailureReason::ActiveEnqueueFailure => {
                eprintln!("hda CoreAudio callback enqueue: state=active reason={} osstatus={} expected=false", reason.label(), status);
                return;
            }
        }
        .fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "hda CoreAudio callback enqueue: state=stopping reason={} osstatus={} expected={}",
            reason.label(),
            status,
            reason.expected()
        );
    }

    pub(super) fn snapshot(&self) -> CallbackFailureSnapshot {
        CallbackFailureSnapshot {
            total: self.total.load(Ordering::Relaxed),
            active: self.active.load(Ordering::Relaxed),
            stopping: self.stopping.load(Ordering::Relaxed),
            expected_stopping: self.expected_stopping.load(Ordering::Relaxed),
            unexpected: self.unexpected.load(Ordering::Relaxed),
            invalid_run_state: self.invalid_run_state.load(Ordering::Relaxed),
            queue_invalidated: self.queue_invalidated.load(Ordering::Relaxed),
            enqueue_during_reset: self.enqueue_during_reset.load(Ordering::Relaxed),
            disposal_pending: self.disposal_pending.load(Ordering::Relaxed),
            stopping_unclassified: self.stopping_unclassified.load(Ordering::Relaxed),
        }
    }

    pub(super) fn print_stats(
        &self,
        frames: u64,
        drops: u64,
        dropped_bytes: u64,
        format_drops: u64,
        ring_full_drops: u64,
        lifecycle: [i32; 2],
    ) {
        let value = self.snapshot();
        for (operation, status) in [("stop", lifecycle[0]), ("dispose", lifecycle[1])] {
            println!(
                "hda CoreAudio lifecycle: operation={operation} osstatus={status} success={}",
                status == 0
            );
        }
        println!(
            "hda CoreAudio stats: frames_rendered={frames} drops={drops} dropped_bytes={dropped_bytes} format_drops={format_drops} ring_full_drops={ring_full_drops} queue_stop_errors={} queue_dispose_errors={} callback_errors={} callback_active_errors={} callback_stopping_errors={} callback_expected_stopping_errors={} callback_unexpected_errors={} callback_stopping_invalid_run_state={} callback_stopping_queue_invalidated={} callback_stopping_enqueue_during_reset={} callback_stopping_disposal_pending={} callback_stopping_unclassified={}",
            u8::from(lifecycle[0] != 0), u8::from(lifecycle[1] != 0),
            value.total, value.active, value.stopping, value.expected_stopping,
            value.unexpected, value.invalid_run_state, value.queue_invalidated,
            value.enqueue_during_reset, value.disposal_pending, value.stopping_unclassified
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_failures_are_always_unexpected_and_keep_the_osstatus_class() {
        let counters = CallbackFailureCounters::new();
        counters.record(INVALID_RUN_STATE);
        assert_eq!(
            counters.snapshot(),
            CallbackFailureSnapshot {
                total: 1,
                active: 1,
                unexpected: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn stopping_allowlist_is_typed_and_expected() {
        let counters = CallbackFailureCounters::new();
        counters.begin_stopping();
        for status in [INVALID_RUN_STATE, ENQUEUE_DURING_RESET, DISPOSAL_PENDING] {
            counters.record(status);
        }
        assert_eq!(
            counters.snapshot(),
            CallbackFailureSnapshot {
                total: 3,
                stopping: 3,
                expected_stopping: 3,
                invalid_run_state: 1,
                enqueue_during_reset: 1,
                disposal_pending: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn queue_invalidation_during_stopping_is_typed_but_unexpected() {
        let counters = CallbackFailureCounters::new();
        counters.begin_stopping();
        counters.record(QUEUE_INVALIDATED);
        assert_eq!(
            counters.snapshot(),
            CallbackFailureSnapshot {
                total: 1,
                stopping: 1,
                unexpected: 1,
                queue_invalidated: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn unknown_stopping_status_fails_closed() {
        let counters = CallbackFailureCounters::new();
        counters.begin_stopping();
        counters.record(-50);
        assert_eq!(
            counters.snapshot(),
            CallbackFailureSnapshot {
                total: 1,
                stopping: 1,
                unexpected: 1,
                stopping_unclassified: 1,
                ..Default::default()
            }
        );
    }
}
