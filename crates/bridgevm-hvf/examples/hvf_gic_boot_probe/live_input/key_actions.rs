use bridgevm_hvf::xhci::SetupInputAction;
use crate::xhci_hid_input::parse_setup_input_actions;

#[path = "key_chords.rs"]
mod key_chords;

/// Setup-input tokens first (text, named keys and their names); any other chord or a bare modifier
/// is one generic report from key_chords. Every other setup error stands.
pub(super) fn parse(value: &str) -> Result<Vec<SetupInputAction>, &'static str> {
    let error = match parse_setup_input_actions(value) {
        Ok(actions) => return Ok(actions),
        Err(error) => error.name(),
    };
    if error != "modifier" && error != "arbitrary_text" {
        return Err(error);
    }
    let (modifier, usage) = key_chords::find(&value.trim().to_ascii_lowercase()).ok_or(error)?;
    Ok(vec![SetupInputAction::Key {
        name: "chord",
        modifier,
        usage,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_chords_preserve_modifiers_and_key_usages() {
        for (token, modifier, usage) in [
            ("shift+left", 0x02, 0x50),
            ("ctrl+right", 0x01, 0x4f),
            ("ctrl+shift+home", 0x03, 0x4a),
            ("shift+tab", 0x02, 0x2b),
            ("shift+enter", 0x02, 0x28),
            ("shift+space", 0x02, 0x2c),
            ("ctrl+c", 0x01, 0x06),
            ("ctrl+v", 0x01, 0x19),
            ("alt+f4", 0x04, 0x3d),
            ("alt+tab", 0x04, 0x2b),
            ("printscreen", 0x00, 0x46),
        ] {
            let actions = parse(token).unwrap();
            assert_eq!(actions.len(), 1);
            assert!(matches!(actions[0], SetupInputAction::Key {
                modifier: m, usage: u, ..
            } if m == modifier && u == usage));
        }
    }

    #[test]
    fn rejects_unknown_chords_and_preserves_setup_limits() {
        for token in [
            "ctrl+usage:0x19",
            "shift+",
            "ctrl+v+v",
            "shift+left,enter",
            "ctrl+shift+alt+delete",
            "shift+tab*2",
            "0x08",
            "text-hex:zz",
            "a",
        ] {
            assert!(parse(token).is_err(), "{token}");
        }
        assert!(parse(&format!("{}shift+tab", " ".repeat(128))).is_err());
        assert!(parse(&format!("{}win", " ".repeat(128))).is_err());
        assert!(parse(&format!("text-hex:{}", "41".repeat(33))).is_err());
        assert!(parse_setup_input_actions("shift+tab").is_err());
    }

    #[test]
    fn preserves_case_sensitive_text_and_setup_names() {
        let actions = parse("text-hex:4161").unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].name(), "A");
        assert_eq!(actions[1].name(), "a");
        assert_eq!(parse("esc").unwrap()[0].name(), "esc");
        assert_eq!(parse("ctrl+alt+delete").unwrap()[0].name(), "ctrl+alt+delete");
        // Generic chords share one name: traces show the report bytes, never a typed key name.
        assert_eq!(parse(" SHIFT+TAB ").unwrap()[0].name(), "chord");
        assert_eq!(parse("win").unwrap()[0].name(), "chord");
    }
}
