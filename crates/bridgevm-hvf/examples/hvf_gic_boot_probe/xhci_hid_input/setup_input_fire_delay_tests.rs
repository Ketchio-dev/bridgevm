use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use super::setup_input::{SetupInputHostWake, XhciSetupInputTrigger};
use super::test_support::{emit_uart, new_platform, ENV_LOCK};
use crate::EXIT_CANCELED;

#[test]
fn xhci_setup_input_fire_delay_waits_until_elapsed_after_marker_seen() {
    // Given: the setup-input marker is visible and fire delay is configured from env.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_FIRE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_FIRE_DELAY";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "50");
    let mut trigger = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env)
        .unwrap()
        .unwrap();
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let start = Instant::now();
    let mut checkpoints = Vec::new();

    // When: setup-input polls before and then at the configured fire delay.
    trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, start, |label: &str| {
        checkpoints.push(label.to_string());
    });
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(49),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );
    let queued_before_elapsed = platform.xhci_setup_input_report_stats().queued_actions;
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(50),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: no report is queued before elapsed, and the trigger fires once at elapsed.
    assert_eq!(queued_before_elapsed, 0);
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!(
        "fire delay queued_before_elapsed={queued_before_elapsed} labels={}",
        checkpoints.join(",")
    );
}

#[test]
fn xhci_setup_input_zero_fire_delay_fires_immediately_from_env() {
    // Given: the setup-input marker is visible and fire delay is explicitly zero.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_ZERO_FIRE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_ZERO_FIRE_DELAY";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "0");
    let mut trigger = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env)
        .unwrap()
        .unwrap();
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: setup-input polls at the first marker-visible instant.
    trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, Instant::now(), |label: &str| {
        checkpoints.push(label.to_string());
    });

    // Then: zero fire delay behaves like the absent env and queues immediately.
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!("zero fire delay labels={}", checkpoints.join(","));
}

#[test]
fn xhci_setup_input_fire_delay_exposes_host_wake_deadline_after_marker_seen() {
    // Given: a delayed setup-input trigger has seen the Windows Setup serial marker.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_HOST_WAKE";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_HOST_WAKE";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "50");
    let mut trigger = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env)
        .unwrap()
        .unwrap();
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    let mut platform = new_platform();
    let start = Instant::now();

    // When: the marker appears but the configured fire delay has not elapsed.
    assert_eq!(
        trigger.pending_host_wake_deadline_at(&platform, start),
        None
    );
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    assert!(!trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, start, |_| {}));

    // Then: the trigger exposes the exact host wake deadline that can re-enter the run loop.
    assert_eq!(
        trigger.pending_host_wake_deadline_at(&platform, start),
        start.checked_add(Duration::from_millis(50))
    );
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 0);
}

#[test]
fn xhci_setup_input_secondary_fire_delay_uses_dedicated_envs() {
    // Given: the primary setup-input delay env is different from a secondary trigger delay.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT2_ACTIONS_FIRE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT2_MARKER_FIRE_DELAY";
    let primary_ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let primary_fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    let secondary_ramfb_delay_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT2_RAMFB_DELAY_MS";
    let secondary_fire_delay_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT2_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "text:g021keys");
    std::env::remove_var(marker_env);
    std::env::set_var(primary_ramfb_delay_env, "5000");
    std::env::set_var(primary_fire_delay_env, "10");
    std::env::set_var(secondary_ramfb_delay_env, "7");
    std::env::set_var(secondary_fire_delay_env, "75");
    let mut trigger = XhciSetupInputTrigger::from_env_with_timing_envs(
        "setup-input-2",
        actions_env,
        marker_env,
        secondary_fire_delay_env,
        secondary_ramfb_delay_env,
    )
    .unwrap()
    .unwrap();
    std::env::remove_var(actions_env);
    std::env::remove_var(primary_ramfb_delay_env);
    std::env::remove_var(primary_fire_delay_env);
    std::env::remove_var(secondary_ramfb_delay_env);
    std::env::remove_var(secondary_fire_delay_env);
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let start = Instant::now();
    let mut checkpoints = Vec::new();

    // When: the trigger polls at the primary delay and then at the secondary delay.
    trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, start, |label: &str| {
        checkpoints.push(label.to_string());
    });
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(10),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );
    let queued_at_primary_delay = platform.xhci_setup_input_report_stats().queued_actions;
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(75),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: only the secondary delay and secondary RAMFB checkpoint config are used.
    assert_eq!(queued_at_primary_delay, 0);
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 8);
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!(
        "secondary fire delay queued_at_primary_delay={queued_at_primary_delay} labels={}",
        checkpoints.join(",")
    );
}

#[test]
fn xhci_setup_input_host_wake_canceled_exit_is_not_watchdog_when_flagged() {
    // Given: a setup-input host wake fired before the watchdog did.
    let wake = SetupInputHostWake::fired_for_test();
    let watchdog_fired = AtomicBool::new(false);

    // When: HVF returns EXIT_CANCELED from the wake.
    let canceled_by_wake = wake.canceled_by_host_wake(EXIT_CANCELED, &watchdog_fired);

    // Then: the canceled exit is consumed as automation, and a second cancel is not.
    assert!(canceled_by_wake);
    assert!(!wake.canceled_by_host_wake(EXIT_CANCELED, &watchdog_fired));
}

#[test]
fn xhci_setup_input_host_wake_arm_invokes_wake_and_marks_canceled_exit() {
    // Given: a setup-input host wake is armed for an immediate deadline.
    let mut wake = SetupInputHostWake::new();
    let callback_fired = Arc::new(AtomicBool::new(false));
    let callback_fired_for_thread = Arc::clone(&callback_fired);
    let now = Instant::now();

    // When: the sleeper thread reaches its deadline.
    assert!(wake.arm(now, move || {
        callback_fired_for_thread.store(true, Ordering::SeqCst);
    }));
    let wait_until = now + Duration::from_secs(1);
    while !callback_fired.load(Ordering::SeqCst) && Instant::now() < wait_until {
        std::thread::yield_now();
    }
    assert!(callback_fired.load(Ordering::SeqCst));

    // Then: the following EXIT_CANCELED is attributed to the setup-input wake.
    let watchdog_fired = AtomicBool::new(false);
    assert!(wake.canceled_by_host_wake(EXIT_CANCELED, &watchdog_fired));
}

#[test]
fn xhci_setup_input_host_wake_does_not_rearm_prior_deadline_after_second_deadline() {
    // Given: two delayed setup-input triggers expose distinct host wake deadlines.
    let mut wake = SetupInputHostWake::new();
    let first_deadline = Instant::now() + Duration::from_secs(60);
    let second_deadline = first_deadline + Duration::from_secs(1);

    // When: both deadlines are armed and the first deadline is observed again.
    assert!(wake.arm(first_deadline, || {}));
    assert!(wake.arm(second_deadline, || {}));

    // Then: the previously armed first deadline is not armed a second time.
    assert!(!wake.arm(first_deadline, || {}));
}

#[test]
fn xhci_setup_input_host_wake_already_passed_deadline_wakes_promptly_and_marks_canceled_exit() {
    // Given: a caller observed a deadline that has already passed by the time host wake is armed.
    let mut wake = SetupInputHostWake::new();
    let callback_fired = Arc::new(AtomicBool::new(false));
    let callback_fired_for_thread = Arc::clone(&callback_fired);
    let deadline = Instant::now() + Duration::from_millis(20);
    while Instant::now() < deadline {
        std::thread::yield_now();
    }

    // When: the host wake is armed after the deadline has passed.
    assert!(wake.arm(deadline, move || {
        callback_fired_for_thread.store(true, Ordering::SeqCst);
    }));
    let wait_until = Instant::now() + Duration::from_millis(100);
    while !callback_fired.load(Ordering::SeqCst) && Instant::now() < wait_until {
        std::thread::yield_now();
    }

    // Then: it wakes promptly and consumes the following EXIT_CANCELED as host wake.
    assert!(callback_fired.load(Ordering::SeqCst));
    let watchdog_fired = AtomicBool::new(false);
    assert!(wake.canceled_by_host_wake(EXIT_CANCELED, &watchdog_fired));
}
