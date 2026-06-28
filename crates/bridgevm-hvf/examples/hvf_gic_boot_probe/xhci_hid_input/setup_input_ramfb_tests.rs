use bridgevm_hvf::xhci::SetupInputAction;
use std::time::{Duration, Instant};

use super::setup_input::XhciSetupInputTrigger;
use super::test_support::{emit_uart, new_platform, ENV_LOCK};

#[test]
fn xhci_setup_input_rejection_does_not_queue_stale_report() {
    let mut platform = new_platform();
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "space",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    trigger.maybe_fire(&mut platform);
    trigger.maybe_fire(&mut platform);

    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
    assert_eq!(platform.xhci_setup_input_report_stats().busy_rejections, 0);
}

#[test]
fn xhci_setup_input_ramfb_skips_checkpoints_after_already_fired() {
    // Given: setup-input has already fired once at the configured marker.
    let mut platform = new_platform();
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "space",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();
    trigger.maybe_fire_with_ramfb_checkpoints(&mut platform, |label: &str| {
        checkpoints.push(label.to_string());
    });
    checkpoints.clear();

    // When: the same trigger is evaluated again after firing.
    trigger.maybe_fire_with_ramfb_checkpoints(&mut platform, |label: &str| {
        checkpoints.push(label.to_string());
    });

    // Then: the already-fired path emits no stale RAMFB checkpoint label.
    assert!(checkpoints.is_empty());
    println!("already-fired ramfb checkpoint labels: none");
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
}

#[test]
fn xhci_setup_input_ramfb_emits_before_after_once_when_trigger_fires() {
    // Given: the setup-input marker has appeared and no xHCI setup reports are pending.
    let mut platform = new_platform();
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "space",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: the setup-input trigger is evaluated twice.
    trigger.maybe_fire_with_ramfb_checkpoints(&mut platform, |label: &str| {
        checkpoints.push(label.to_string());
    });
    trigger.maybe_fire_with_ramfb_checkpoints(&mut platform, |label: &str| {
        checkpoints.push(label.to_string());
    });

    // Then: the successful one-shot fire is bounded by exactly one before/after pair.
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!("ramfb checkpoint labels: {}", checkpoints.join(","));
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
}

#[test]
fn xhci_setup_input_ramfb_emits_each_delay_checkpoint_once_after_elapsed() {
    // Given: setup-input has two configured delayed RAMFB checkpoints.
    let mut platform = new_platform();
    let mut trigger =
        XhciSetupInputTrigger::from_env_value_with_ramfb_delay_ms("setup-input", "space", &[5, 15])
            .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let start = Instant::now();
    let mut checkpoints = Vec::new();

    // When: setup-input fires and the live loop polls before, at, and after each delay.
    trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, start, |label: &str| {
        checkpoints.push(label.to_string());
    });
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(4),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(5),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(20),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(25),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: immediate labels are unchanged and each delayed label appears exactly once.
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string(),
            "setup-input-delay-5ms".to_string(),
            "setup-input-delay-15ms".to_string()
        ]
    );
    println!("delay checkpoint labels: {}", checkpoints.join(","));
}

#[test]
fn xhci_setup_input_ramfb_default_delay_env_emits_task_owned_labels() {
    // Given: setup-input is created from env with the delay env absent.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_DEFAULT_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_DEFAULT_DELAY";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(delay_env);
    std::env::remove_var(fire_delay_env);
    let mut trigger = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env)
        .unwrap()
        .unwrap();
    std::env::remove_var(actions_env);
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let start = Instant::now();
    let mut checkpoints = Vec::new();

    // When: the default task-owned delay schedule elapses.
    trigger.maybe_fire_with_ramfb_checkpoints_at(&mut platform, start, |label: &str| {
        checkpoints.push(label.to_string());
    });
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        start + Duration::from_millis(15_000),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: the default delay labels are emitted once after setup-input fires.
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string(),
            "setup-input-delay-1000ms".to_string(),
            "setup-input-delay-5000ms".to_string(),
            "setup-input-delay-15000ms".to_string()
        ]
    );
    println!("delay checkpoint labels: {}", checkpoints.join(","));
}

#[test]
fn xhci_setup_input_ramfb_skips_checkpoints_when_marker_is_absent() {
    // Given: a valid setup-input trigger whose serial marker has not appeared.
    let mut platform = new_platform();
    let mut trigger = XhciSetupInputTrigger::from_env_value("setup-input", "space").unwrap();
    let mut checkpoints = Vec::new();

    // When: setup-input is evaluated before the marker.
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        Instant::now() + Duration::from_millis(15_000),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: no stale RAMFB checkpoint label is emitted.
    assert!(checkpoints.is_empty());
    println!("absent marker ramfb checkpoint labels: none");
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 0);
}

#[test]
fn xhci_setup_input_ramfb_skips_checkpoints_when_queue_rejects_trigger() {
    // Given: the marker is visible but a previous setup-input report is still pending.
    let mut platform = new_platform();
    platform
        .queue_xhci_setup_input_actions(&[SetupInputAction::Space])
        .unwrap();
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "enter",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: setup-input reaches the marker but the queue rejects the new action.
    trigger.maybe_fire_with_ramfb_checkpoints_at(
        &mut platform,
        Instant::now() + Duration::from_millis(15_000),
        |label: &str| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: the rejected trigger does not emit stale before/after RAMFB checkpoints.
    assert!(checkpoints.is_empty());
    println!("busy ramfb checkpoint labels: none");
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 1);
    assert_eq!(platform.xhci_setup_input_report_stats().busy_rejections, 1);
}
