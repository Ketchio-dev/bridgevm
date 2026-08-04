//! Interleaving model of the wake coordinator's claim/request race.
//!
//! Wakers publish a reason and then cancel; the run loop claims whatever is
//! pending. Two properties have to hold under every interleaving:
//!
//!   1. A reason published before a claim is never lost. Losing one turns a
//!      legitimate wake into an apparent surplus cancel, which is exactly the
//!      signal the A1 investigation reads.
//!   2. A claim never reports a reason twice. Double-counting would invent
//!      wake pressure that did not happen.
//!
//! Run with:
//!   RUSTFLAGS="--cfg loom" cargo test --test loom_reset_generation --release
#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// The coordinator's pending bitset, modelled exactly: `fetch_or` to publish,
/// `swap(0)` to claim.
struct Pending(AtomicU64);

impl Pending {
    fn new() -> Self {
        Self(AtomicU64::new(0))
    }
    fn request(&self, reason: u64) {
        self.0.fetch_or(reason, Ordering::SeqCst);
    }
    fn claim(&self) -> u64 {
        self.0.swap(0, Ordering::SeqCst)
    }
}

#[test]
fn a_published_reason_is_never_lost() {
    loom::model(|| {
        let pending = Arc::new(Pending::new());
        let publisher = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.request(0b01))
        };
        let claimer = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.claim())
        };

        publisher.join().unwrap();
        let claimed = claimer.join().unwrap();
        // Either the claim ran first and saw nothing (the reason is still
        // pending), or it ran after and collected it. It can never vanish.
        let still_pending = pending.claim();
        assert_eq!(
            claimed | still_pending,
            0b01,
            "the published reason must be observable exactly once"
        );
    });
}

#[test]
fn two_reasons_collapse_into_at_most_one_wake_and_none_is_dropped() {
    loom::model(|| {
        let pending = Arc::new(Pending::new());
        let first = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.request(0b01))
        };
        let second = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.request(0b10))
        };
        let claimer = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.claim())
        };

        first.join().unwrap();
        second.join().unwrap();
        let claimed = claimer.join().unwrap();
        let leftover = pending.claim();
        assert_eq!(
            claimed | leftover,
            0b11,
            "both reasons must survive to exactly one claim between them"
        );
        assert_eq!(
            claimed & leftover,
            0,
            "a reason must never be reported by two claims"
        );
    });
}

#[test]
fn two_claims_never_report_the_same_reason() {
    // Double-counting would invent wake pressure that never happened, which
    // is the number the stall investigation reads.
    loom::model(|| {
        let pending = Arc::new(Pending::new());
        pending.request(0b100);

        let first = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.claim())
        };
        let second = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.claim())
        };

        let a = first.join().unwrap();
        let b = second.join().unwrap();
        assert_eq!(a & b, 0, "no reason may be claimed twice");
        assert_eq!(a | b, 0b100, "the reason must be claimed once");
    });
}

#[test]
fn a_generation_bump_and_a_claim_cannot_both_consume_a_request() {
    // A reboot clears pending. Whether the claim or the clear wins, the
    // reason must not be observed by both.
    loom::model(|| {
        let pending = Arc::new(Pending::new());
        pending.request(0b01);

        let claimer = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.claim())
        };
        let rebooter = {
            let pending = Arc::clone(&pending);
            thread::spawn(move || pending.0.swap(0, Ordering::SeqCst))
        };

        let claimed = claimer.join().unwrap();
        let cleared = rebooter.join().unwrap();
        assert_eq!(
            claimed & cleared,
            0,
            "the request must be consumed by exactly one of the two"
        );
        assert_eq!(claimed | cleared, 0b01);
    });
}
