use super::setup_input::{XhciSetupInputEnvError, XhciSetupInputTrigger};

#[test]
fn xhci_setup_input_accepts_minimal_navigation_tokens() {
    let trigger = XhciSetupInputTrigger::from_env_value("setup-input", "tab,enter space").unwrap();

    assert_eq!(trigger.action_names(), "tab,enter,space");
}

#[test]
fn xhci_setup_input_accepts_desktop_typing_sequence() {
    let trigger = XhciSetupInputTrigger::from_env_value(
        "setup-input",
        "win+r text:notepad enter text:g021keys",
    )
    .unwrap();

    assert_eq!(
        trigger.action_names(),
        "win+r,n,o,t,e,p,a,d,enter,g,0,2,1,k,e,y,s"
    );
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
fn xhci_setup_input_rejects_unsupported_text_characters() {
    assert_eq!(
        XhciSetupInputTrigger::from_env_value("setup-input", "text:hello!").unwrap_err(),
        XhciSetupInputEnvError::UnsupportedToken {
            token: "text:hello!".to_string()
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
