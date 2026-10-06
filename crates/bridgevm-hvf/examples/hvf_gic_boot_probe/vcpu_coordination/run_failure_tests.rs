use super::*;

fn secondary_set() -> SecondaryVcpuSet {
    SecondaryVcpuSet {
        shutdown: Arc::new(AtomicBool::new(false)),
        terminal: Arc::new(SecondaryTerminalSignal::new()),
        controls: vec![Arc::new(VcpuControl::new(1))],
        handles: Vec::new(),
    }
}

#[test]
fn fatal_secondary_blocks_the_next_primary_entry() {
    let set = secondary_set();
    set.controls[0].run_error.store(true, Ordering::SeqCst);
    let entered = AtomicBool::new(false);
    let result = run_primary_with_secondary_check(Some(&set), || {
        entered.store(true, Ordering::SeqCst);
        Ok(EXIT_EXCEPTION)
    });
    assert_eq!(result, Err(PrimaryRunError::Secondary(1)));
    assert!(!entered.load(Ordering::SeqCst));
}

#[test]
fn failure_during_primary_run_is_observed_before_dispatch() {
    let set = secondary_set();
    let result = run_primary_with_secondary_check(Some(&set), || {
        set.controls[0].run_error.store(true, Ordering::SeqCst);
        Ok(EXIT_CANCELED)
    });
    assert_eq!(result, Err(PrimaryRunError::Secondary(1)));
}

#[test]
fn secondary_publishes_fatal_failure_before_waking_primary() {
    let set = secondary_set();
    let wakes = AtomicU64::new(0);
    set.controls[0].record_run_error_with(|| {
        assert!(set.controls[0].run_error.load(Ordering::SeqCst));
        assert_eq!(set.failed_cpu(), Some(1));
        wakes.fetch_add(1, Ordering::SeqCst);
        0
    });
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(set.terminal.action(), None);
}

#[test]
fn withdrawing_failed_cpu_does_not_hide_fatal_failure() {
    let set = secondary_set();
    let control = &set.controls[0];
    *control.state.lock().unwrap() = PsciState::On;
    control.publish_vcpu(0x1234);
    control.record_run_error_with(|| 0);
    control.withdraw_vcpu(0x1234);
    assert_eq!(*control.vcpu.lock().unwrap(), None);
    assert_eq!(set.failed_cpu(), Some(1));
}

#[test]
fn healthy_secondary_and_single_cpu_preserve_primary_exit() {
    let set = secondary_set();
    assert_eq!(
        run_primary_with_secondary_check(Some(&set), || Ok(EXIT_EXCEPTION)),
        Ok(EXIT_EXCEPTION)
    );
    assert_eq!(
        run_primary_with_secondary_check(None, || Ok(EXIT_CANCELED)),
        Ok(EXIT_CANCELED)
    );
}

#[test]
fn actual_hvf_error_status_is_preserved_even_if_secondary_fails() {
    let set = secondary_set();
    let actual_status = -7;
    let result = run_primary_with_secondary_check(Some(&set), || {
        set.controls[0].run_error.store(true, Ordering::SeqCst);
        Err(actual_status)
    });
    assert_eq!(result, Err(PrimaryRunError::Hypervisor(actual_status)));
    assert_eq!(
        result.unwrap_err().to_string(),
        format!("hv_vcpu_run error {actual_status:#x}")
    );
}

#[test]
fn guest_psci_terminal_request_is_not_a_fatal_run_failure() {
    for (function, action) in [
        (PSCI_SYSTEM_OFF, PsciTerminalAction::SystemOff),
        (PSCI_SYSTEM_RESET, PsciTerminalAction::SystemReset),
    ] {
        let set = secondary_set();
        assert!(set.terminal.record(function));
        assert_eq!(
            run_primary_with_secondary_check(Some(&set), || Ok(EXIT_CANCELED)),
            Ok(EXIT_CANCELED)
        );
        assert_eq!(set.terminal_action(), Some(action));
        assert_eq!(set.failed_cpu(), None);
    }
}

#[test]
fn failed_wake_keeps_fatal_failure_visible_without_fabricating_run_status() {
    let set = secondary_set();
    let simulated_wake_status = -9;
    let status = set.controls[0].record_run_error_with(|| simulated_wake_status);
    assert_eq!(status, simulated_wake_status);
    assert_eq!(set.failed_cpu(), Some(1));
    let result = run_primary_with_secondary_check(Some(&set), || panic!("entered after failure"));
    assert_eq!(result, Err(PrimaryRunError::Secondary(1)));
    assert_eq!(set.terminal_action(), None);
}

#[test]
fn fatal_secondary_is_not_overridden_by_a_guest_reset_request() {
    let set = secondary_set();
    set.terminal.record(PSCI_SYSTEM_RESET);
    set.controls[0].record_run_error_with(|| 0);
    let result = run_primary_with_secondary_check(Some(&set), || panic!("entered after failure"));
    assert_eq!(result, Err(PrimaryRunError::Secondary(1)));
    assert_eq!(set.terminal_action(), Some(PsciTerminalAction::SystemReset));
}
