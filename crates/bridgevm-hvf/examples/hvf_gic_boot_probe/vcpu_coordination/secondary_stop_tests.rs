use super::super::run_failure::{run_primary_with_secondary_check, PrimaryRunError};
use super::*;

fn unexpected_owner_exit(cause: SecondaryUnexpectedStop) {
    let control = Arc::new(VcpuControl::new(1));
    *control.state.lock().unwrap() = PsciState::On;
    control.publish_vcpu(0x1234);
    let owner = Arc::clone(&control);
    let wakes = Arc::new(AtomicU64::new(0));
    let owner_wakes = Arc::clone(&wakes);
    let (withdrawn_tx, withdrawn_rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let stopped = finish_unexpected_secondary_stop(owner.index, cause, || {
            let status = owner.record_run_error_with(|| {
                assert!(
                    owner.run_error.load(Ordering::SeqCst),
                    "wake preceded publication"
                );
                owner_wakes.fetch_add(1, Ordering::SeqCst);
                -9
            });
            assert_eq!(
                status, -9,
                "wake result must not become a fabricated run status"
            );
        });
        assert!(stopped);
        assert!(owner.publish_final_state(VcpuFinalState::test_state(owner.index)));
        owner.withdraw_vcpu(0x1234);
        withdrawn_tx.send(()).unwrap();
    });
    withdrawn_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let set = SecondaryVcpuSet {
        shutdown: Arc::new(AtomicBool::new(false)),
        terminal: Arc::new(SecondaryTerminalSignal::new()),
        controls: vec![Arc::clone(&control)],
        handles: vec![handle],
    };
    let entered = AtomicBool::new(false);
    let primary_result = run_primary_with_secondary_check(Some(&set), || {
        entered.store(true, Ordering::SeqCst);
        Ok(EXIT_CANCELED)
    });
    let state = *control.state.lock().unwrap();
    let published = *control.vcpu.lock().unwrap();
    assert!(set.terminal.record(PSCI_SYSTEM_RESET));
    let joined = set.shutdown_and_join();
    let decisions: Vec<_> = [false, true]
        .into_iter()
        .map(|exit_on_reset| {
            let (mut fatal, mut reset, mut reason) = (false, true, "PSCI system reset".to_string());
            joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
            (
                fatal,
                reset.then(|| {
                    decide_system_reset(
                        0,
                        RebootPlan {
                            max_reboots: 8,
                            exit_on_reset,
                        },
                    )
                }),
            )
        })
        .collect();
    assert!(joined.run_error, "unexpected owner exit was accepted: state={state:?} published={published:?} wakes={} primary={primary_result:?} entered={} decisions={decisions:?}", wakes.load(Ordering::SeqCst), entered.load(Ordering::SeqCst));
    assert_eq!(state, PsciState::On, "do not fabricate guest CPU_OFF");
    assert_eq!(published, None);
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    assert_eq!(primary_result, Err(PrimaryRunError::Secondary(1)));
    assert!(!entered.load(Ordering::SeqCst));
    assert_eq!(decisions, [(true, None), (true, None)]);
    assert!(joined.missing_final_states.is_empty());
    assert_eq!(joined.final_states.len(), 1);
    assert_eq!(joined.final_states[0].psci_state, PsciState::On);
}

#[test]
fn unexpected_secondary_exit_reason_blocks_primary_and_both_reset_modes() {
    unexpected_owner_exit(SecondaryUnexpectedStop::ExitReason(99));
}
#[test]
fn unsupported_secondary_sysreg_blocks_primary_and_both_reset_modes() {
    unexpected_owner_exit(SecondaryUnexpectedStop::SysReg(
        SysRegTrap::decode(0x6030_0015),
        0x6030_0015,
        0x1234,
    ));
}
#[test]
fn unhandled_secondary_exception_blocks_primary_and_both_reset_modes() {
    unexpected_owner_exit(SecondaryUnexpectedStop::Exception(
        0x3f,
        0xfc00_0000,
        0x1234,
    ));
}
#[test]
fn secondary_exit_cap_blocks_primary_and_both_reset_modes() {
    unexpected_owner_exit(SecondaryUnexpectedStop::ExitCap(7));
}

#[test]
fn normal_shutdown_and_psci_terminal_requests_remain_nonfatal() {
    for function in [PSCI_SYSTEM_OFF, PSCI_SYSTEM_RESET] {
        let control = Arc::new(VcpuControl::new(1));
        *control.state.lock().unwrap() = PsciState::On;
        let set = SecondaryVcpuSet {
            shutdown: Arc::new(AtomicBool::new(false)),
            terminal: Arc::new(SecondaryTerminalSignal::new()),
            controls: vec![Arc::clone(&control)],
            handles: Vec::new(),
        };
        assert!(set.terminal.record(function));
        assert_eq!(
            run_primary_with_secondary_check(Some(&set), || Ok(EXIT_CANCELED)),
            Ok(EXIT_CANCELED)
        );
        let joined = set.shutdown_and_join();
        let (mut fatal, mut reset, mut reason) = (
            false,
            function == PSCI_SYSTEM_RESET,
            "guest terminal".to_string(),
        );
        joined.merge_run_error(&mut fatal, &mut reset, &mut reason);
        assert!(!fatal);
        assert_eq!(reset, function == PSCI_SYSTEM_RESET);
        assert!(!control.run_error.load(Ordering::SeqCst));
    }
}
