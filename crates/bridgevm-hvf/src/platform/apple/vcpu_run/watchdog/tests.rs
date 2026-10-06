use super::*;
use std::cell::Cell;

#[test]
fn completed_before_wait_does_not_cancel() {
    let done = AtomicBool::new(true);
    assert_eq!(
        cancel_after_timeout(&done, 3, |_| panic!("already complete"), || 0),
        None
    );
}

#[test]
fn completion_during_an_earlier_wait_does_not_cancel() {
    let done = AtomicBool::new(false);
    let waits = Cell::new(0);
    let result = cancel_after_timeout(
        &done,
        3,
        |_| {
            waits.set(waits.get() + 1);
            done.store(true, Ordering::SeqCst);
        },
        || panic!("completed before deadline"),
    );
    assert_eq!(result, None::<i32>);
    assert_eq!(waits.get(), 1);
}

#[test]
fn completion_during_the_final_wait_does_not_cancel() {
    let done = AtomicBool::new(false);
    let waits = Cell::new(0);
    let cancels = Cell::new(0);
    let result = cancel_after_timeout(
        &done,
        3,
        |duration| {
            assert_eq!(duration, Duration::from_millis(1));
            waits.set(waits.get() + 1);
            if waits.get() == 3 {
                done.store(true, Ordering::SeqCst);
            }
        },
        || {
            cancels.set(cancels.get() + 1);
            0
        },
    );
    assert_eq!((result, cancels.get(), waits.get()), (None, 0, 3));
}

#[test]
fn zero_timeout_keeps_one_wait_and_honors_completion() {
    let done = AtomicBool::new(false);
    let waits = Cell::new(0);
    let cancels = Cell::new(0);
    let result = cancel_after_timeout(
        &done,
        0,
        |_| {
            waits.set(waits.get() + 1);
            done.store(true, Ordering::SeqCst);
        },
        || {
            cancels.set(cancels.get() + 1);
            -7
        },
    );
    assert_eq!((result, cancels.get(), waits.get()), (None, 0, 1));
}

#[test]
fn active_timeout_cancels_once_and_preserves_provider_status() {
    for status in [0, -7] {
        let done = AtomicBool::new(false);
        let waits = Cell::new(0);
        let cancels = Cell::new(0);
        let result = cancel_after_timeout(
            &done,
            2,
            |_| waits.set(waits.get() + 1),
            || {
                cancels.set(cancels.get() + 1);
                status
            },
        );
        assert_eq!((result, cancels.get(), waits.get()), (Some(status), 1, 2));
    }
}
