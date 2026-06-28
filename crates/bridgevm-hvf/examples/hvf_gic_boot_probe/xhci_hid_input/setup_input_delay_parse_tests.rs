use super::setup_input::{XhciSetupInputEnvError, XhciSetupInputTrigger};
use super::test_support::ENV_LOCK;

#[test]
fn xhci_setup_input_delay_parse_rejection_has_no_ramfb_checkpoint_trigger() {
    // Given: setup-input is valid but delayed RAMFB checkpoint config is malformed.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_DELAY_REJECT";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_DELAY_REJECT";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::set_var(delay_env, "1000,nope");
    std::env::remove_var(fire_delay_env);

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: delay parse rejection prevents any trigger from existing.
    std::env::remove_var(actions_env);
    std::env::remove_var(delay_env);
    println!("parse rejection delay: 1000,nope");
    assert_eq!(trigger_result.unwrap_err().name(), "ramfb_delay_invalid");
}

#[test]
fn xhci_setup_input_empty_delay_env_has_no_ramfb_checkpoint_trigger() {
    // Given: setup-input is valid but delayed RAMFB checkpoint config is empty.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_EMPTY_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_EMPTY_DELAY";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::set_var(delay_env, " ");
    std::env::remove_var(fire_delay_env);

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: delay parse rejection prevents any trigger from existing.
    std::env::remove_var(actions_env);
    std::env::remove_var(delay_env);
    println!("parse rejection delay: empty");
    assert_eq!(trigger_result.unwrap_err().name(), "ramfb_delay_empty");
}

#[test]
fn xhci_setup_input_overlarge_delay_env_has_no_ramfb_checkpoint_trigger() {
    // Given: setup-input is valid but delayed RAMFB checkpoint config is overlarge.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_OVERLARGE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_OVERLARGE_DELAY";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::set_var(delay_env, "120001");
    std::env::remove_var(fire_delay_env);

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: delay parse rejection prevents any trigger from existing.
    std::env::remove_var(actions_env);
    std::env::remove_var(delay_env);
    println!("parse rejection delay: overlarge");
    assert_eq!(trigger_result.unwrap_err().name(), "ramfb_delay_too_large");
}

#[test]
fn xhci_setup_input_invalid_fire_delay_env_has_no_trigger() {
    // Given: setup-input is valid but fire-delay config is malformed.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_FIRE_DELAY_REJECT";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_FIRE_DELAY_REJECT";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "nope");

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: fire-delay parse rejection prevents any trigger from existing.
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    println!("parse rejection fire delay: nope");
    assert_eq!(trigger_result.unwrap_err().name(), "fire_delay_invalid");
}

#[test]
fn xhci_setup_input_overlarge_fire_delay_env_has_no_trigger() {
    // Given: setup-input is valid but fire-delay config is overlarge.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_OVERLARGE_FIRE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_OVERLARGE_FIRE_DELAY";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "600001");

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: fire-delay parse rejection prevents any trigger from existing.
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    println!("parse rejection fire delay: overlarge");
    assert_eq!(trigger_result.unwrap_err().name(), "fire_delay_too_large");
}

#[test]
fn xhci_setup_input_install_scale_fire_delay_env_creates_trigger() {
    // Given: setup-input uses a fire delay long enough for the Windows Setup UI.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_INSTALL_FIRE_DELAY";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_INSTALL_FIRE_DELAY";
    let ramfb_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(ramfb_delay_env);
    std::env::set_var(fire_delay_env, "170000");

    // When: setup-input follows the same env parse branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();

    // Then: the live-install delay is accepted instead of capped at RAMFB checkpoint limits.
    std::env::remove_var(actions_env);
    std::env::remove_var(fire_delay_env);
    assert!(trigger_result.is_ok());
    println!("install-scale fire delay accepted: 170000");
}

#[test]
fn xhci_setup_input_rejects_duplicate_delay_tokens() {
    // Given: setup-input is configured with two identical valid delay tokens.
    let result =
        XhciSetupInputTrigger::from_env_value_with_ramfb_delay_ms("setup-input", "space", &[5, 5]);

    // Then: duplicate delay labels are rejected instead of emitted twice.
    assert_eq!(
        result.unwrap_err(),
        XhciSetupInputEnvError::RamfbDelayDuplicate { delay_ms: 5 }
    );
    println!("duplicate delay parse rejection: 5");
}
