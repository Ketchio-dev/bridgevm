//! Concurrency invariants of the store. Split out of part_3_2.rs.

use super::super::helpers::*;

#[test]
fn concurrent_creates_of_one_name_produce_exactly_one_vm() {
    // create_vm used to check `exists()` and then build the bundle, so two
    // callers racing on a name both passed the check and the loser overwrote
    // the winner's manifest and guest-tools token. Two threads succeeded
    // together in 40 of 40 rounds; the bundle directory is now reserved
    // atomically, so exactly one caller wins.
    use std::sync::{Arc, Barrier};

    for _ in 0..20 {
        let store = Arc::new(temp_store());
        store.ensure().unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let racers: Vec<_> = (0..2)
            .map(|_| {
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.create_vm(&manifest("dup")).is_ok()
                })
            })
            .collect();
        let winners = racers
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|created| *created)
            .count();
        assert_eq!(winners, 1, "exactly one concurrent create must succeed");
    }
}
