use super::*;

#[test]
fn a_fresh_coordinator_has_nothing_pending() {
    let coordinator = WakeCoordinator::new();
    assert_eq!(coordinator.generation(), 0);
    assert_eq!(coordinator.counters(), (0, 0, 0, 0));
}

#[test]
fn a_published_reason_is_claimed_by_the_next_cancel() {
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::Vblank);
    let claim = coordinator.claim();
    assert_eq!(claim.reasons(), vec![WakeReason::Vblank]);
    assert_eq!(coordinator.counters(), (1, 1, 0, 0));
}

#[test]
fn a_cancel_with_no_reason_pending_is_surplus() {
    // This is the exit the A1 investigation could previously only guess at.
    let coordinator = WakeCoordinator::new();
    assert_eq!(coordinator.claim(), CancelClaim::Surplus);
    assert_eq!(coordinator.counters(), (0, 0, 1, 0));
}

#[test]
fn several_reasons_collapse_into_one_wake() {
    // Wakers race; the vCPU comes back once. All pending reasons must be
    // reported together rather than leaving some unclaimed for a later cancel
    // to pick up and look legitimate.
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::RamfbSample);
    coordinator.request(WakeReason::AgentConsole);
    coordinator.request(WakeReason::Vblank);
    let reasons = coordinator.claim().reasons();
    assert_eq!(reasons.len(), 3);
    assert!(reasons.contains(&WakeReason::RamfbSample));
    assert!(reasons.contains(&WakeReason::AgentConsole));
    assert!(reasons.contains(&WakeReason::Vblank));
}

#[test]
fn the_second_cancel_after_a_collapsed_wake_is_surplus() {
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::RamfbSample);
    coordinator.request(WakeReason::Vblank);
    assert!(matches!(coordinator.claim(), CancelClaim::Claimed(_)));
    assert_eq!(
        coordinator.claim(),
        CancelClaim::Surplus,
        "two requests, one wake, so the second cancel claims nothing"
    );
    assert_eq!(coordinator.counters(), (2, 1, 1, 0));
}

#[test]
fn the_same_reason_requested_twice_claims_once() {
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::Vblank);
    coordinator.request(WakeReason::Vblank);
    assert_eq!(coordinator.claim().reasons(), vec![WakeReason::Vblank]);
    assert_eq!(coordinator.claim(), CancelClaim::Surplus);
}

#[test]
fn each_generation_is_observed_once_and_increases() {
    let coordinator = WakeCoordinator::new();
    assert_eq!(coordinator.begin_generation(), 1);
    assert_eq!(coordinator.begin_generation(), 2);
    assert_eq!(coordinator.generation(), 2);
}

#[test]
fn a_new_generation_discards_the_previous_generations_request() {
    // A reboot recreates the vCPU. A cancellation asked for by the old one
    // must not be counted as an answer for the new one.
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::RebootWatchdog);
    coordinator.begin_generation();
    assert_eq!(
        coordinator.claim(),
        CancelClaim::Surplus,
        "the stale request was dropped, so this cancel claims nothing"
    );
}

#[test]
fn a_request_recorded_against_an_old_generation_is_reported_stale() {
    let coordinator = WakeCoordinator::new();
    coordinator.request(WakeReason::RamfbSample);
    // Simulate the reboot landing between the request and the claim, without
    // clearing pending: the generation tag is what catches it.
    coordinator.generation.fetch_add(1, Ordering::SeqCst);
    assert_eq!(coordinator.claim(), CancelClaim::StaleGeneration);
    assert_eq!(coordinator.counters().3, 1);
}

#[test]
fn requests_report_the_generation_they_were_recorded_against() {
    let coordinator = WakeCoordinator::new();
    assert_eq!(coordinator.request(WakeReason::Vblank), 0);
    coordinator.begin_generation();
    assert_eq!(coordinator.request(WakeReason::Vblank), 1);
}

#[test]
fn every_reason_has_a_distinct_bit_and_a_name() {
    let mut seen = 0u64;
    for reason in WakeReason::ALL {
        let bit = reason as u64;
        assert_eq!(bit.count_ones(), 1, "{reason:?} must be a single bit");
        assert_eq!(seen & bit, 0, "{reason:?} collides with another reason");
        seen |= bit;
        assert!(!reason.as_str().is_empty());
    }
}

#[test]
fn concurrent_wakers_never_lose_a_reason_or_double_count_a_claim() {
    use std::sync::Arc;
    let coordinator = Arc::new(WakeCoordinator::new());
    let mut handles = Vec::new();
    for reason in WakeReason::ALL {
        let coordinator = Arc::clone(&coordinator);
        handles.push(std::thread::spawn(move || {
            for _ in 0..50 {
                coordinator.request(reason);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let (requests, ..) = coordinator.counters();
    assert_eq!(requests, 350);
    // One claim collects whatever is pending; nothing may be left behind.
    assert!(matches!(coordinator.claim(), CancelClaim::Claimed(_)));
    assert_eq!(coordinator.claim(), CancelClaim::Surplus);
}

#[test]
fn a_surplus_claim_reports_no_reasons() {
    assert!(CancelClaim::Surplus.reasons().is_empty());
    assert!(CancelClaim::StaleGeneration.reasons().is_empty());
}
