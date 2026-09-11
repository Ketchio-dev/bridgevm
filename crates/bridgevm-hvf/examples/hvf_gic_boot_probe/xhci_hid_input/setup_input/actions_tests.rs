use super::parse_setup_input_actions;
use bridgevm_hvf::xhci::SetupInputAction;

#[test]
fn text_hex_preserves_case_space_and_shifted_symbols() {
    let actions = parse_setup_input_actions("text-hex:41612040213f2c").unwrap();
    let expected = [
        ("A", 0x02, 0x04),
        ("a", 0x00, 0x04),
        ("space", 0x00, 0x2c),
        ("@", 0x02, 0x1f),
        ("!", 0x02, 0x1e),
        ("?", 0x02, 0x38),
        (",", 0x00, 0x36),
    ];
    assert_eq!(actions.len(), expected.len());
    for (action, (name, modifier, usage)) in actions.iter().zip(expected) {
        assert_eq!(action.name(), name);
        assert_eq!(action.usage(), usage);
        assert!(matches!(
            action,
            SetupInputAction::Key { modifier: actual, .. } if *actual == modifier
        ));
    }
}

#[test]
fn text_hex_rejects_invalid_hex_control_bytes_and_more_than_32_keys() {
    assert!(parse_setup_input_actions("text-hex:0g").is_err());
    assert!(parse_setup_input_actions("text-hex:0a").is_err());
    assert!(parse_setup_input_actions(&format!("text-hex:{}", "41".repeat(33))).is_err());
}

#[test]
fn named_special_keys_map_to_boot_keyboard_usages_and_modifiers() {
    let actions = parse_setup_input_actions(
        "esc,backspace,delete,left,right,up,down,home,end,pageup,pagedown,ctrl+alt+delete",
    )
    .unwrap();
    let expected = [
        (0x29, 0x00),
        (0x2a, 0x00),
        (0x4c, 0x00),
        (0x50, 0x00),
        (0x4f, 0x00),
        (0x52, 0x00),
        (0x51, 0x00),
        (0x4a, 0x00),
        (0x4d, 0x00),
        (0x4b, 0x00),
        (0x4e, 0x00),
        (0x4c, 0x05),
    ];
    assert_eq!(actions.len(), expected.len());
    for (action, (usage, modifier)) in actions.iter().zip(expected) {
        assert_eq!(action.usage(), usage);
        assert!(matches!(
            action,
            SetupInputAction::Key { modifier: actual, .. } if *actual == modifier
        ));
    }
}

#[test]
fn firmware_function_keys_map_to_boot_keyboard_usages() {
    let actions = parse_setup_input_actions("f1,f2,f3,f4,f5,f6,f7,f8,f9,f10,f11,f12").unwrap();

    assert_eq!(actions.len(), 12);
    for (index, action) in actions.iter().enumerate() {
        assert_eq!(action.name(), format!("f{}", index + 1));
        assert_eq!(action.usage(), 0x3a + u8::try_from(index).unwrap());
    }
}

#[test]
fn clipboard_paste_chord_maps_to_control_v_without_enabling_arbitrary_chords() {
    for token in ["ctrl+v", "CTRL+V"] {
        let actions = parse_setup_input_actions(token).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], SetupInputAction::Key {
            name: "ctrl+v", modifier: 0x01, usage: 0x19
        }));
    }
    for token in ["ctrl+", "ctrl+vv", "ctrl+v+v", "ctrl+usage:0x19", "ctrl+0x19"] {
        assert!(parse_setup_input_actions(token).is_err());
    }
}
