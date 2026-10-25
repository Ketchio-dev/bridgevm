//! How the HDA service tick's thread waits: parked until its next deadline
//! unless a stop is requested, at user-interactive QoS (hda_service_tick.rs).

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::thread;
use std::time::Instant;

/// Park until `deadline`; false as soon as a stop is requested.
pub(super) fn sleep_until(deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(SeqCst) {
            return false;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        thread::park_timeout(left);
    }
}

/// User-interactive QoS for the calling thread, so its 5 ms wakes are prompt.
pub(super) fn raise_qos() {
    use libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE;
    // SAFETY: sets only the calling thread's own QoS class; no pointers.
    let status = unsafe { libc::pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
    if status != 0 {
        eprintln!("hda service tick: QoS not raised: {status}");
    }
}
