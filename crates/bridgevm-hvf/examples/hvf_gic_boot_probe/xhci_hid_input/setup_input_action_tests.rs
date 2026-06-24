use super::setup_input::{XhciSetupInputEnvError, XhciSetupInputTrigger};

#[test]
fn xhci_setup_input_accepts_minimal_navigation_tokens() {
    let trigger = XhciSetupInputTrigger::from_env_value("setup-input", "tab,enter space").unwrap();

    assert_eq!(trigger.action_names(), "tab,enter,space");
}

#[test]
fn xhci_setup_input_rejects_arbitrary_text() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "hello").unwrap_err(),
        XhciSetupInputEnvError::ArbitraryText {
            token: "hello".to_string()
        }
    );
}

#[test]
fn xhci_setup_input_rejects_modifiers() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "shift+tab").unwrap_err(),
        XhciSetupInputEnvError::Modifier {
            token: "shift+tab".to_string()
        }
    );
}

#[test]
fn xhci_setup_input_rejects_repeats() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "tab*3").unwrap_err(),
        XhciSetupInputEnvError::Repeat {
            token: "tab*3".to_string()
        }
    );
}

#[test]
fn xhci_setup_input_rejects_unsupported_tokens_and_usages() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "esc").unwrap_err(),
        XhciSetupInputEnvError::UnsupportedToken {
            token: "esc".to_string()
        }
    );
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "0x04").unwrap_err(),
        XhciSetupInputEnvError::UnsupportedUsage {
            token: "0x04".to_string()
        }
    );
}
