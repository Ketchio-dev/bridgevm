//! Interleaving model of the PSCI CPU_ON race.
//!
//! `psci::cpu_on_decision` is pure, but the run loop applies it under a mutex
//! and then transitions the state. The property that matters is that two
//! callers racing to start the same CPU cannot both be told SUCCESS: a guest
//! that believes it started a CPU twice waits forever for a second entry that
//! never happens.
//!
//! An ordinary test observes one interleaving of many and would pass against a
//! check-then-act bug. loom enumerates them. Run with:
//!   RUSTFLAGS="--cfg loom" cargo test --test loom_psci --release
#![cfg(loom)]

use loom::sync::{Arc, Mutex};
use loom::thread;

/// The probe's PSCI states, mirrored here so this model does not depend on the
/// example crate's private types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Off,
    OnPending,
    On,
}

/// The critical section exactly as `psci_cpu_on` performs it: check and
/// transition inside one lock acquisition.
fn cpu_on(state: &Mutex<State>) -> bool {
    let mut guard = state.lock().unwrap();
    match *guard {
        State::Off => {
            *guard = State::OnPending;
            true
        }
        State::OnPending | State::On => false,
    }
}

#[test]
fn two_racing_cpu_on_calls_produce_exactly_one_success() {
    loom::model(|| {
        let state = Arc::new(Mutex::new(State::Off));
        let first = {
            let state = Arc::clone(&state);
            thread::spawn(move || cpu_on(&state))
        };
        let second = {
            let state = Arc::clone(&state);
            thread::spawn(move || cpu_on(&state))
        };

        let successes = usize::from(first.join().unwrap()) + usize::from(second.join().unwrap());
        assert_eq!(
            successes, 1,
            "exactly one caller may be told the CPU was started"
        );
        assert_eq!(*state.lock().unwrap(), State::OnPending);
    });
}

#[test]
fn a_cpu_already_on_is_never_started_again() {
    loom::model(|| {
        let state = Arc::new(Mutex::new(State::On));
        let first = {
            let state = Arc::clone(&state);
            thread::spawn(move || cpu_on(&state))
        };
        let second = {
            let state = Arc::clone(&state);
            thread::spawn(move || cpu_on(&state))
        };
        assert!(!first.join().unwrap());
        assert!(!second.join().unwrap());
        assert_eq!(*state.lock().unwrap(), State::On);
    });
}

#[test]
fn a_started_cpu_never_falls_back_to_off() {
    // The started CPU moves OnPending -> On itself. A caller arriving during
    // that window must not be able to leave the CPU stopped.
    loom::model(|| {
        let state = Arc::new(Mutex::new(State::Off));
        let starter = {
            let state = Arc::clone(&state);
            thread::spawn(move || cpu_on(&state))
        };
        let secondary = {
            let state = Arc::clone(&state);
            thread::spawn(move || {
                let mut guard = state.lock().unwrap();
                if *guard == State::OnPending {
                    *guard = State::On;
                }
            })
        };
        starter.join().unwrap();
        secondary.join().unwrap();
        let final_state = *state.lock().unwrap();
        assert!(
            matches!(final_state, State::OnPending | State::On),
            "a started CPU never falls back to Off, saw {final_state:?}"
        );
    });
}
