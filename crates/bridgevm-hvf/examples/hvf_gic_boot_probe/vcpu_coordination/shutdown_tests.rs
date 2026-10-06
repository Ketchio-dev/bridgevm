//! Shutdown notifications must survive the Off predicate-to-wait boundary.

use crate::*;
use std::sync::mpsc::{channel, RecvTimeoutError};

const NOTIFIER_WINDOW: Duration = Duration::from_millis(500);
const CLEANUP_BOUND: Duration = Duration::from_secs(2);

#[test]
fn shutdown_notification_cannot_pass_an_off_waiters_predicate_lock() {
    let control = Arc::new(VcpuControl::new(1));
    let shutdown = Arc::new(AtomicBool::new(false));
    let state = control.state.lock().unwrap();
    assert_eq!(*state, PsciState::Off);
    assert!(!shutdown.load(Ordering::SeqCst));
    assert_eq!(*control.vcpu.lock().unwrap(), None);

    let (entered_tx, entered_rx) = channel();
    let (notified_tx, notified_rx) = channel();
    let notifier_control = Arc::clone(&control);
    let notifier_shutdown = Arc::clone(&shutdown);
    let notifier = thread::spawn(move || {
        notifier_shutdown.store(true, Ordering::SeqCst);
        let _ = entered_tx.send(());
        notifier_control.notify_shutdown();
        let _ = notified_tx.send(());
    });
    let entered = entered_rx.recv_timeout(CLEANUP_BOUND);
    let early_notification = notified_rx.recv_timeout(NOTIFIER_WINDOW);
    // The predicate was already checked above. This models the exact window
    // before the production owner atomically unlocks and starts waiting.
    let (state, waited) = control.condvar.wait_timeout(state, CLEANUP_BOUND).unwrap();
    let observed_state = *state;
    drop(state);
    let notifier_result = notifier.join();

    // Assert only after releasing the lock and joining the helper, including
    // the broken case where the notification was lost before wait started.
    assert!(notifier_result.is_ok());
    assert!(entered.is_ok(), "notifier never entered the measured window");
    assert!(
        matches!(early_notification, Err(RecvTimeoutError::Timeout)),
        "notification completed while the Off waiter still held its predicate lock: {early_notification:?}; wait_timed_out={}",
        waited.timed_out()
    );
    assert!(!waited.timed_out(), "shutdown notification did not reach the waiter");
    assert!(shutdown.load(Ordering::SeqCst));
    assert_eq!(observed_state, PsciState::Off);
}

fn joined_no_handle_owner(shutdown_before_wait: bool) {
    let control = Arc::new(VcpuControl::new(1));
    let shutdown = Arc::new(AtomicBool::new(shutdown_before_wait));
    let owner_control = Arc::clone(&control);
    let owner_shutdown = Arc::clone(&shutdown);
    let (ready_tx, ready_rx) = channel();
    let (result_tx, result_rx) = channel();
    let owner = thread::spawn(move || {
        let state = owner_control.state.lock().unwrap();
        let _ = ready_tx.send(());
        let (state, waited) = owner_control.condvar.wait_timeout_while(
            state,
            CLEANUP_BOUND,
            |state| *state == PsciState::Off && !owner_shutdown.load(Ordering::SeqCst),
        ).unwrap();
        let observed = (*state, waited.timed_out());
        drop(state);
        let _ = result_tx.send(observed);
    });
    let ready = ready_rx.recv_timeout(CLEANUP_BOUND);
    // Acquiring this after the ready message proves that the owner released
    // its predicate lock into wait (or skipped waiting after prior shutdown).
    drop(control.state.lock().unwrap());
    let set = SecondaryVcpuSet {
        shutdown,
        terminal: Arc::new(SecondaryTerminalSignal::new()),
        controls: vec![Arc::clone(&control)],
        handles: vec![owner],
    };
    let stopped = set.shutdown_and_join();
    let observed = result_rx.recv_timeout(CLEANUP_BOUND);

    assert!(ready.is_ok());
    assert_eq!(observed.unwrap(), (PsciState::Off, false));
    assert!(!stopped.run_error);
    assert_eq!(stopped.exit_counts, vec![(1, 0)]);
    assert!(stopped.final_states.is_empty());
    assert!(stopped.missing_final_states.is_empty());
    assert_eq!(*control.vcpu.lock().unwrap(), None);
    assert!(!control.created.load(Ordering::Acquire));
}

#[test]
fn shutdown_and_join_wakes_an_already_parked_owner_without_an_hvf_handle() {
    joined_no_handle_owner(false);
}

#[test]
fn shutdown_before_the_off_predicate_needs_no_notification() {
    joined_no_handle_owner(true);
}

#[test]
fn shutdown_notification_preserves_psci_state() {
    let control = VcpuControl::new(1);
    for state in [PsciState::Off, PsciState::OnPending, PsciState::On] {
        *control.state.lock().unwrap() = state;
        control.notify_shutdown();
        assert_eq!(*control.state.lock().unwrap(), state);
    }
}

#[test]
fn shutdown_notification_still_wakes_other_controls_after_a_poisoned_state() {
    let poisoned = VcpuControl::new(1);
    let poisoned_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = poisoned.state.lock().unwrap();
        panic!("fixture poisons the predicate mutex");
    }));
    assert!(poisoned_result.is_err());
    assert!(poisoned.state.is_poisoned());
    let notification = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        poisoned.notify_shutdown();
        // This invokes actual shutdown/join for another parked owner, proving
        // that notification-only locking did not abort before later wakes.
        joined_no_handle_owner(false);
    }));
    assert!(notification.is_ok(), "poison prevented later shutdown notification");
    assert!(poisoned.state.is_poisoned(), "notification must not clear poison");
}
