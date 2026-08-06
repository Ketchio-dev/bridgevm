use super::*;

#[test]
fn an_event_from_the_current_boot_is_current() {
    let generation = ResetGeneration::new();
    let tag = generation.stamp();
    assert!(generation.is_current(tag));
}

#[test]
fn a_reset_makes_every_earlier_stamp_stale() {
    let generation = ResetGeneration::new();
    let before = generation.stamp();
    let after = generation.advance();
    assert!(
        !generation.is_current(before),
        "pre-reset event must be stale"
    );
    assert!(generation.is_current(after));
}

#[test]
fn a_stale_tag_never_becomes_current_again() {
    // The counter is monotonic: no sequence of resets can resurrect an old
    // tag, which is what would re-open the stale-delivery bug.
    let generation = ResetGeneration::new();
    let old = generation.stamp();
    for _ in 0..3 {
        generation.advance();
        assert!(!generation.is_current(old));
    }
}

#[test]
fn concurrent_stamps_and_resets_never_blur_generations() {
    use std::sync::Arc;
    let generation = Arc::new(ResetGeneration::new());
    let stampers: Vec<_> = (0..4)
        .map(|_| {
            let g = Arc::clone(&generation);
            std::thread::spawn(move || (0..1000).map(|_| g.stamp()).collect::<Vec<_>>())
        })
        .collect();
    let resetter = {
        let g = Arc::clone(&generation);
        std::thread::spawn(move || {
            for _ in 0..100 {
                g.advance();
            }
        })
    };
    resetter.join().expect("resetter");
    for handle in stampers {
        for tag in handle.join().expect("stamper") {
            // After all resets, only the final generation may be current.
            assert_eq!(generation.is_current(tag), tag == generation.stamp());
        }
    }
}
