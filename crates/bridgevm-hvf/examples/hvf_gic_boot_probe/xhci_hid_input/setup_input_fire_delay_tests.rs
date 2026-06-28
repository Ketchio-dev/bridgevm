use std::time::{Duration, Instant};

use super::setup_input::XhciSetupInputTrigger;
use super::test_support::{emit_uart, new_platform, ENV_LOCK};

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
