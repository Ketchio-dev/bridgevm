use super::*;

fn decide_after_join(run_error: bool, plan: RebootPlan) -> Option<SystemResetDecision> {
    // CPU0 already accepted SYSTEM_RESET; the fatal flag becomes visible only
    // when a secondary finishes and the owner joins it.
    let joined = SecondaryVcpuStopResult {
        run_error,
        ..Default::default()
    };
    let (mut fatal, mut reset) = (false, true);
    let mut reason = format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)");
    joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
    assert_eq!(fatal, run_error);
    reset.then(|| decide_system_reset(0, plan))
}

#[test]
fn late_joined_failure_prevents_in_process_reboot() {
    assert_eq!(
        decide_after_join(
            true,
            RebootPlan {
                max_reboots: 8,
                exit_on_reset: false
            }
        ),
        None
    );
}

#[test]
fn late_joined_failure_prevents_process_recreation() {
    assert_eq!(
        decide_after_join(
            true,
            RebootPlan {
                max_reboots: 8,
                exit_on_reset: true
            }
        ),
        None
    );
}

#[test]
fn joined_secondary_failure_reports_error_instead_of_guest_shutdown() {
    let joined = SecondaryVcpuStopResult {
        run_error: true,
        ..Default::default()
    };
    let (mut fatal, mut reset) = (false, false);
    let mut reason = format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)");
    joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
    assert!(fatal);
    assert!(!reset);
    assert_eq!(reason, "secondary vCPU fatal run error");
}

#[test]
fn joined_failure_preserves_already_selected_primary_hvf_error() {
    let joined = SecondaryVcpuStopResult {
        run_error: true,
        ..Default::default()
    };
    let (mut fatal, mut reset) = (true, true);
    let prior = format!("hv_vcpu_run error {:#x}", -7);
    let mut reason = prior.clone();
    joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
    assert!(fatal);
    assert!(!reset);
    assert_eq!(reason, prior);
}

#[test]
fn healthy_join_preserves_both_reset_policy_modes() {
    assert!(matches!(
        decide_after_join(
            false,
            RebootPlan {
                max_reboots: 8,
                exit_on_reset: false
            }
        ),
        Some(SystemResetDecision::Reboot { .. })
    ));
    assert_eq!(
        decide_after_join(
            false,
            RebootPlan {
                max_reboots: 8,
                exit_on_reset: true
            }
        ),
        Some(SystemResetDecision::ExitForRecreate)
    );
}

#[test]
fn extracted_secondary_psci_reason_preserves_terminal_request() {
    for (function, reset, name) in [
        (PSCI_SYSTEM_OFF, false, "system off"),
        (PSCI_SYSTEM_RESET, true, "system reset"),
    ] {
        let set = SecondaryVcpuSet {
            shutdown: Arc::new(AtomicBool::new(false)),
            terminal: Arc::new(SecondaryTerminalSignal::new()),
            controls: Vec::new(),
            handles: Vec::new(),
        };
        assert_eq!(set.psci_stop_reason(), None);
        set.terminal.record(function);
        assert_eq!(
            set.psci_stop_reason(),
            Some((format!("PSCI {function:#x} ({name})"), reset))
        );
    }
}

#[test]
fn existing_primary_fatal_error_suppresses_reset_even_with_healthy_join() {
    let joined = SecondaryVcpuStopResult::default();
    let (mut fatal, mut reset) = (true, true);
    let prior = format!("hv_vcpu_run error {:#x}", -7);
    let mut reason = prior.clone();
    joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
    assert!(fatal);
    assert!(!reset);
    assert_eq!(reason, prior);
}
