use std::time::Instant;

use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::setup_input::{XhciSetupInputEnvError, XhciSetupInputTrigger};
use super::test_support::{
    configure_dci3_interrupt_in_over_bar0, emit_uart, new_platform_and_ram, program_xhci_bar0,
    read_bytes, write_dci3_normal_trb, DCI3_KEY_BUFFER, DCI3_RING, TRB_SIZE,
};

#[test]
fn xhci_setup_input_accepts_minimal_navigation_tokens() {
    let trigger = XhciSetupInputTrigger::from_env_value("setup-input", "tab,enter,space").unwrap();

    assert_eq!(trigger.action_names(), "tab,enter,space");
}

#[test]
fn xhci_setup_input_accepts_desktop_typing_sequence() {
    let trigger = XhciSetupInputTrigger::from_env_value(
        "setup-input",
        "win+r,text:notepad,enter,text:g021keys",
    )
    .unwrap();

    assert_eq!(
        trigger.action_names(),
        "win+r,n,o,t,e,p,a,d,enter,g,0,2,1,k,e,y,s"
    );
}

#[test]
fn xhci_setup_input_accepts_shutdown_command_tokens() {
    let trigger = XhciSetupInputTrigger::from_env_value(
        "setup-input",
        "text:shutdown,space,text:/s,space,text:/t,space,text:0",
    )
    .unwrap();

    assert_eq!(
        trigger.action_names(),
        "s,h,u,t,d,o,w,n,space,/,s,space,/,t,space,0"
    );
}

#[test]
fn xhci_setup_input_text_punctuation_maps_to_hid_usages() {
    let expected_usages = [0x04, 0x38, 0x05, 0x2d, 0x06, 0x37, 0x07];
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);

    for report_index in 0..(expected_usages.len() * 2) {
        let report_index = u64::try_from(report_index).unwrap();
        let buffer = DCI3_KEY_BUFFER + (report_index * 0x20);
        write_dci3_normal_trb(&mut mem, DCI3_RING + (TRB_SIZE * report_index), buffer);
        assert!(mem.write_bytes(buffer, &[0xaa; 8]));
    }

    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "text:a/b-c.d",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    assert!(trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        Instant::now(),
        |_label, _mem| {},
    ));

    assert_eq!(trigger.action_names(), "a,/,b,-,c,.,d");
    for (action_index, usage) in expected_usages.iter().copied().enumerate() {
        let press_index = u64::try_from(action_index * 2).unwrap();
        let release_index = press_index + 1;
        assert_eq!(
            read_bytes(&mem, DCI3_KEY_BUFFER + (press_index * 0x20), 8),
            [0, 0, usage, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            read_bytes(&mem, DCI3_KEY_BUFFER + (release_index * 0x20), 8),
            [0; 8]
        );
    }

    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 7);
    assert_eq!(stats.queued_reports, 14);
    assert_eq!(stats.emitted_key_reports, 7);
    assert_eq!(stats.emitted_release_reports, 7);
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
    for value in ["text:A", "text:hello there", "text:hello!"] {
        assert_eq!(
            XhciSetupInputTrigger::from_env_value("setup-input", value).unwrap_err(),
            XhciSetupInputEnvError::UnsupportedToken {
                token: value.to_string()
            }
        );
    }
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
