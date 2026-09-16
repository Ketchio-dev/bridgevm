use super::*;
use std::os::unix::process::ExitStatusExt;
fn start(role: ChildRole, pid: u32, generation: Option<u64>) -> RuntimeLifecycleEvent {
    RuntimeLifecycleEvent::ChildStarted {
        role,
        pid,
        generation,
    }
}
fn reap(role: ChildRole, pid: u32, generation: Option<u64>, code: i32) -> RuntimeLifecycleEvent {
    RuntimeLifecycleEvent::ChildReaped {
        role,
        pid,
        generation,
        status: std::process::ExitStatus::from_raw(code << 8),
    }
}
#[test]
fn all_reset_generations_and_tpm_must_be_reaped() {
    let mut ledger = Ledger::default();
    ledger.observe(start(ChildRole::Swtpm, 100, None));
    for generation in 0..3 {
        ledger.observe(start(
            ChildRole::Helper,
            101 + generation as u32,
            Some(generation),
        ));
        assert!(!ledger.complete());
        ledger.observe(reap(
            ChildRole::Helper,
            101 + generation as u32,
            Some(generation),
            if generation < 2 { 42 } else { 0 },
        ));
    }
    assert!(!ledger.complete());
    ledger.observe(reap(ChildRole::Swtpm, 100, None, 0));
    assert!(ledger.complete());
    assert_eq!(ledger.helper.spawned_count, 3);
    assert_eq!(ledger.helper.reaped_count, 3);
    assert_eq!(ledger.helper.last.as_ref().unwrap().generation, Some(2));
}
#[test]
fn mismatched_duplicate_or_missing_observations_never_complete() {
    for fact in [
        reap(ChildRole::Helper, 5, Some(0), 0),
        start(ChildRole::Helper, 5, Some(1)),
    ] {
        let mut ledger = Ledger::default();
        ledger.observe(fact);
        assert!(!ledger.complete());
    }
    let mut ledger = Ledger::default();
    ledger.observe(start(ChildRole::Helper, 5, Some(0)));
    ledger.observe(reap(ChildRole::Helper, 5, Some(0), 0));
    assert!(ledger.complete());
    ledger.observe(reap(ChildRole::Helper, 5, Some(0), 0));
    assert!(!ledger.complete());
}
