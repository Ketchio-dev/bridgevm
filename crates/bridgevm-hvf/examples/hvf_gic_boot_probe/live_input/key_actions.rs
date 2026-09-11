use bridgevm_hvf::xhci::SetupInputAction;
use crate::xhci_hid_input::parse_setup_input_actions;

const CHORDS: &[(&str, u8, u8)] = &[
    ("shift+left", 0x02, 0x50),
    ("ctrl+left", 0x01, 0x50),
    ("ctrl+shift+left", 0x03, 0x50),
    ("shift+right", 0x02, 0x4f),
    ("ctrl+right", 0x01, 0x4f),
    ("ctrl+shift+right", 0x03, 0x4f),
    ("shift+up", 0x02, 0x52),
    ("ctrl+up", 0x01, 0x52),
    ("ctrl+shift+up", 0x03, 0x52),
    ("shift+down", 0x02, 0x51),
    ("ctrl+down", 0x01, 0x51),
    ("ctrl+shift+down", 0x03, 0x51),
    ("shift+home", 0x02, 0x4a),
    ("ctrl+home", 0x01, 0x4a),
    ("ctrl+shift+home", 0x03, 0x4a),
    ("shift+end", 0x02, 0x4d),
    ("ctrl+end", 0x01, 0x4d),
    ("ctrl+shift+end", 0x03, 0x4d),
    ("shift+pageup", 0x02, 0x4b),
    ("ctrl+pageup", 0x01, 0x4b),
    ("ctrl+shift+pageup", 0x03, 0x4b),
    ("shift+pagedown", 0x02, 0x4e),
    ("ctrl+pagedown", 0x01, 0x4e),
    ("ctrl+shift+pagedown", 0x03, 0x4e),
    ("shift+tab", 0x02, 0x2b),
    ("ctrl+a", 0x01, 0x4),
    ("ctrl+c", 0x01, 0x6),
    ("ctrl+x", 0x01, 0x1b),
    ("ctrl+z", 0x01, 0x1d),
    ("ctrl+y", 0x01, 0x1c),
    ("ctrl+backspace", 0x01, 0x2a),
    ("ctrl+delete", 0x01, 0x4c),
    ("alt+tab", 0x04, 0x2b),
    ("alt+f4", 0x04, 0x3d),
    ("alt+enter", 0x04, 0x28),
];

pub(super) fn parse(value: &str) -> Result<Vec<SetupInputAction>, &'static str> {
    let error = match parse_setup_input_actions(value) {
        Ok(actions) => return Ok(actions),
        Err(error) => error.name(),
    };
    if error != "modifier" {
        return Err(error);
    }
    let normalized = value.trim().to_ascii_lowercase();
    let Some(&(name, modifier, usage)) = CHORDS.iter().find(|(name, _, _)| *name == normalized) else {
        return Err(error);
    };
    Ok(vec![SetupInputAction::Key { name, modifier, usage }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_chords_preserve_modifiers_and_key_usages() {
        for (token, modifier, usage) in [
            ("shift+left", 0x02, 0x50), ("ctrl+right", 0x01, 0x4f),
            ("ctrl+shift+home", 0x03, 0x4a), ("shift+tab", 0x02, 0x2b),
            ("ctrl+c", 0x01, 0x06), ("ctrl+v", 0x01, 0x19),
            ("alt+f4", 0x04, 0x3d), ("alt+tab", 0x04, 0x2b),
        ] {
            let actions = parse(token).unwrap();
            assert_eq!(actions.len(), 1);
            assert!(matches!(actions[0], SetupInputAction::Key {
                modifier: m, usage: u, ..
            } if m == modifier && u == usage));
        }
        for &(token, _, _) in CHORDS {
            assert_eq!(parse(token).unwrap().len(), 1);
        }
    }

    #[test]
    fn rejects_unknown_chords_and_preserves_setup_limits() {
        for token in ["ctrl+usage:0x19", "shift+", "ctrl+v+v", "shift+left,enter",
                      "ctrl+shift+alt+delete", "shift+tab*2"] {
            assert!(parse(token).is_err(), "{token}");
        }
        assert!(parse(&format!("{}shift+tab", " ".repeat(128))).is_err());
        assert!(parse(&format!("text-hex:{}", "41".repeat(33))).is_err());
        assert!(parse_setup_input_actions("shift+tab").is_err());
    }

    #[test]
    fn preserves_case_sensitive_text_and_normalizes_named_chords() {
        let actions = parse("text-hex:4161").unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].name(), "A");
        assert_eq!(actions[1].name(), "a");
        assert_eq!(parse(" SHIFT+TAB ").unwrap()[0].name(), "shift+tab");
    }
}
