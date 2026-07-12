use super::marker::{MarkerEnvError, MARKER_MAX_BYTES};
use super::setup_input::{
    print_setup_input_rejection, XhciSetupInputEnvError, XhciSetupInputTrigger,
};
use super::test_support::{emit_uart, new_platform, ENV_LOCK};

#[test]
fn xhci_setup_input_uses_default_marker_when_env_is_absent() {
    let trigger = XhciSetupInputTrigger::from_env_value("setup-input", "space").unwrap();

    assert_eq!(trigger.marker().source_name(), "default");
    assert_eq!(trigger.marker().as_bytes(), b"BdsDxe: starting Boot");
}

#[test]
fn xhci_setup_input_from_env_defaults_absent_marker() {
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_DEFAULT_MARKER";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_ABSENT";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::remove_var(marker_env);
    std::env::remove_var(delay_env);
    std::env::remove_var(fire_delay_env);

    let result = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env);

    std::env::remove_var(actions_env);
    let trigger = result.unwrap().unwrap();
    assert_eq!(trigger.marker().source_name(), "default");
}

#[cfg(unix)]
#[test]
fn xhci_setup_input_from_env_rejects_non_unicode_marker() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_NONUNICODE";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_NONUNICODE";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "space");
    std::env::set_var(marker_env, OsString::from_vec(vec![0xff]));
    std::env::remove_var(delay_env);
    std::env::remove_var(fire_delay_env);

    let result = XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env);

    std::env::remove_var(actions_env);
    std::env::remove_var(marker_env);
    assert_eq!(
        result.unwrap().unwrap_err(),
        XhciSetupInputEnvError::Marker(MarkerEnvError::NotUnicode {
            env_name: marker_env
        })
    );
}

#[test]
fn xhci_setup_input_rejects_empty_marker() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value_with_custom_marker("setup-input", "space", b"")
            .unwrap_err(),
        XhciSetupInputEnvError::Marker(MarkerEnvError::Empty)
    );
}

#[test]
fn xhci_setup_input_rejects_overlong_marker() {
    let marker = vec![b'a'; MARKER_MAX_BYTES + 1];

    assert_eq!(
        XhciSetupInputTrigger::from_env_value_with_custom_marker("setup-input", "space", &marker,)
            .unwrap_err(),
        XhciSetupInputEnvError::Marker(MarkerEnvError::TooLong {
            len: MARKER_MAX_BYTES + 1,
            max: MARKER_MAX_BYTES,
        })
    );
}

#[test]
fn xhci_setup_input_parse_rejection_has_no_ramfb_checkpoint_trigger() {
    // Given: env-facing setup-input text that is rejected while the marker is already visible.
    let _guard = ENV_LOCK.lock().unwrap();
    let actions_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_ACTIONS_PARSE_REJECT";
    let marker_env = "BRIDGEVM_TEST_XHCI_SETUP_INPUT_MARKER_PARSE_REJECT";
    let delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS";
    let fire_delay_env = "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS";
    std::env::set_var(actions_env, "hello");
    std::env::remove_var(marker_env);
    std::env::remove_var(delay_env);
    std::env::remove_var(fire_delay_env);
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    // When: setup-input follows the same parse-result branch as the boot probe.
    let trigger_result =
        XhciSetupInputTrigger::from_env("setup-input", actions_env, marker_env).unwrap();
    std::env::remove_var(actions_env);
    let mut triggers = Vec::new();
    match trigger_result {
        Ok(trigger) => triggers.push(trigger),
        Err(error) => {
            assert_eq!(
                error,
                XhciSetupInputEnvError::ArbitraryText {
                    token: "hello".to_string()
                }
            );
            print_setup_input_rejection("setup-input", &error);
        }
    }
    let mut checkpoints = Vec::new();
    for trigger in &mut triggers {
        trigger.maybe_fire_with_ramfb_checkpoints(&mut platform, |label: &str| {
            checkpoints.push(label.to_string());
        });
    }

    // Then: parse rejection leaves no trigger that can emit before/after RAMFB checkpoints.
    assert!(checkpoints.is_empty());
    assert_eq!(platform.xhci_setup_input_report_stats().queued_actions, 0);
}
