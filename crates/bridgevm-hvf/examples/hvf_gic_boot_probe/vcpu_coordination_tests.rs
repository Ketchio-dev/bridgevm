use super::*;

#[test]
fn vcpu_control_starts_off_with_linear_mpidr() {
    let control = VcpuControl::new(17);

    assert_eq!(*control.state.lock().unwrap(), PsciState::Off);
    assert_eq!(control.entry.load(Ordering::SeqCst), 0);
    assert_eq!(control.context.load(Ordering::SeqCst), 0);
    assert_eq!(*control.vcpu.lock().unwrap(), None);
    assert_eq!(control.index, 17);
    assert_eq!(control.mpidr, 0x8000_0000 | machine::cpu_mpidr(17));
    assert!(control.take_final_state().is_none());
}

#[test]
fn psci_state_has_parked_secondary_transitions_reserved() {
    let control = VcpuControl::new(1);
    {
        let mut state = control.state.lock().unwrap();
        *state = PsciState::OnPending;
        assert_eq!(*state, PsciState::OnPending);
        *state = PsciState::On;
        assert_eq!(*state, PsciState::On);
        *state = PsciState::Off;
    }
    assert_eq!(*control.state.lock().unwrap(), PsciState::Off);
}

#[test]
fn secondary_vcpu_handle_cannot_withdraw_during_shutdown_action() {
    let control = Arc::new(VcpuControl::new(1));
    let fake_handle = 0x1234;
    let (action_entered_tx, action_entered_rx) = std::sync::mpsc::channel();
    let (release_action_tx, release_action_rx) = std::sync::mpsc::channel();
    let (withdraw_started_tx, withdraw_started_rx) = std::sync::mpsc::channel();
    let (withdraw_done_tx, withdraw_done_rx) = std::sync::mpsc::channel();

    control.publish_vcpu(fake_handle);
    let action_control = Arc::clone(&control);
    let action_thread = thread::spawn(move || {
        let _ = action_control.with_published_vcpu(|vcpu| {
            assert_eq!(vcpu, fake_handle);
            action_entered_tx.send(()).unwrap();
            release_action_rx.recv().unwrap();
        });
    });
    action_entered_rx.recv().unwrap();

    let withdraw_control = Arc::clone(&control);
    let withdraw_thread = thread::spawn(move || {
        withdraw_started_tx.send(()).unwrap();
        withdraw_control.withdraw_vcpu(fake_handle);
        withdraw_done_tx.send(()).unwrap();
    });
    withdraw_started_rx.recv().unwrap();
    let withdrawal_while_locked = withdraw_done_rx.recv_timeout(Duration::from_millis(25));

    release_action_tx.send(()).unwrap();
    action_thread.join().unwrap();
    if withdrawal_while_locked.is_err() {
        withdraw_done_rx.recv().unwrap();
    }
    withdraw_thread.join().unwrap();
    assert!(
        matches!(
            withdrawal_while_locked,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "withdrawal completed while a shutdown action held the handle"
    );
    assert_eq!(*control.vcpu.lock().unwrap(), None);
}

#[test]
fn final_state_publication_is_one_shot_and_take_is_destructive() {
    let control = VcpuControl::new(2);
    let state = VcpuFinalState::test_state(2);

    control.publish_final_state(state.clone()).unwrap();
    assert_eq!(control.publish_final_state(state.clone()), Err(state.clone()));
    assert_eq!(control.take_final_state(), Some(state));
    assert!(control.take_final_state().is_none());
}

#[test]
fn created_secondary_without_snapshot_is_reported_missing() {
    let control = Arc::new(VcpuControl::new(1));
    control.created.store(true, Ordering::Release);
    let set = SecondaryVcpuSet {
        shutdown: Arc::new(AtomicBool::new(false)),
        terminal: Arc::new(SecondaryTerminalSignal::new()),
        controls: vec![control],
        handles: Vec::new(),
    };

    let result = set.shutdown_and_join();

    assert!(result.final_states.is_empty());
    assert_eq!(result.missing_final_states, vec![1]);
}

#[test]
fn secondary_terminal_signal_preserves_the_first_system_request() {
    let signal = SecondaryTerminalSignal::new();

    assert_eq!(signal.action(), None);
    assert!(signal.record(PSCI_SYSTEM_OFF));
    assert!(!signal.record(PSCI_SYSTEM_RESET));
    assert_eq!(signal.action(), Some(PsciTerminalAction::SystemOff));
}

#[test]
fn secondary_terminal_signal_accepts_a_system_reset_request() {
    let signal = SecondaryTerminalSignal::new();

    assert!(signal.record(PSCI_SYSTEM_RESET));
    assert_eq!(signal.action(), Some(PsciTerminalAction::SystemReset));
}
