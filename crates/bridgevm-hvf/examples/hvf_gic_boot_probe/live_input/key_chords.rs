//! Ctrl+A-Z, Ctrl+Enter, Ctrl+Tab, Ctrl+Shift+Tab, Shift+F1-F12 and Shift+Delete on the legacy
//! route. The VMM writes these boot-keyboard reports itself; no guest agent is involved.

const CTRL_LETTERS: [&str; 26] = [
    "ctrl+a", "ctrl+b", "ctrl+c", "ctrl+d", "ctrl+e", "ctrl+f", "ctrl+g", "ctrl+h", "ctrl+i",
    "ctrl+j", "ctrl+k", "ctrl+l", "ctrl+m", "ctrl+n", "ctrl+o", "ctrl+p", "ctrl+q", "ctrl+r",
    "ctrl+s", "ctrl+t", "ctrl+u", "ctrl+v", "ctrl+w", "ctrl+x", "ctrl+y", "ctrl+z",
];
const SHIFT_FUNCTIONS: [&str; 12] = [
    "shift+f1",
    "shift+f2",
    "shift+f3",
    "shift+f4",
    "shift+f5",
    "shift+f6",
    "shift+f7",
    "shift+f8",
    "shift+f9",
    "shift+f10",
    "shift+f11",
    "shift+f12",
];
const NAMED: &[(&str, u8, u8)] = &[
    ("ctrl+enter", 0x01, 0x28),
    ("ctrl+tab", 0x01, 0x2b),
    ("ctrl+shift+tab", 0x03, 0x2b),
    ("shift+delete", 0x02, 0x4c),
];

/// Boot-keyboard modifier bits (0x01 left Ctrl, 0x02 left Shift) and HID keyboard usages:
/// letters start at 0x04 (A) and function keys at 0x3a (F1).
pub(super) fn find(name: &str) -> Option<(&'static str, u8, u8)> {
    if let Some(index) = CTRL_LETTERS.iter().position(|chord| *chord == name) {
        return Some((CTRL_LETTERS[index], 0x01, 0x04 + index as u8));
    }
    if let Some(index) = SHIFT_FUNCTIONS.iter().position(|chord| *chord == name) {
        return Some((SHIFT_FUNCTIONS[index], 0x02, 0x3a + index as u8));
    }
    NAMED.iter().copied().find(|(chord, _, _)| *chord == name)
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::find;
    use bridgevm_hvf::xhci::SetupInputAction;

    #[test]
    fn legacy_key_commands_reach_every_ctrl_letter_and_chord_as_one_report() {
        for (token, modifier, usage) in [
            ("ctrl+a", 0x01, 0x04),
            ("ctrl+s", 0x01, 0x16),
            (" CTRL+W ", 0x01, 0x1a),
            ("ctrl+v", 0x01, 0x19),
            ("ctrl+y", 0x01, 0x1c),
            ("ctrl+enter", 0x01, 0x28),
            ("ctrl+tab", 0x01, 0x2b),
            ("ctrl+shift+tab", 0x03, 0x2b),
            ("shift+f10", 0x02, 0x43),
            ("shift+delete", 0x02, 0x4c),
        ] {
            let actions = parse(token).unwrap();
            assert!(
                matches!(actions.as_slice(), [SetupInputAction::Key { modifier: m, usage: u, .. }]
                    if *m == modifier && *u == usage),
                "{token}"
            );
        }
        for token in [
            "ctrl+shift+s",
            "shift+s",
            "alt+s",
            "ctrl+1",
            "shift+f13",
            "ctrl+s,ctrl+s",
        ] {
            assert!(parse(token).is_err(), "{token}");
        }
    }

    #[test]
    fn every_ctrl_letter_uses_left_ctrl_and_its_letter_usage() {
        for (index, letter) in ('a'..='z').enumerate() {
            let token = format!("ctrl+{letter}");
            let (name, modifier, usage) = find(&token).unwrap();
            assert_eq!(
                (name, modifier, usage),
                (token.as_str(), 0x01, 0x04 + index as u8)
            );
        }
        assert_eq!(find("ctrl+s"), Some(("ctrl+s", 0x01, 0x16)));
        assert_eq!(find("ctrl+w"), Some(("ctrl+w", 0x01, 0x1a)));
    }

    #[test]
    fn shift_function_keys_delete_and_ctrl_enter_tab_match_hid_usages() {
        for number in 1..=12u8 {
            let token = format!("shift+f{number}");
            assert_eq!(find(&token), Some((token.as_str(), 0x02, 0x39 + number)));
        }
        for (token, modifier, usage) in [
            ("shift+f10", 0x02, 0x43),
            ("shift+delete", 0x02, 0x4c),
            ("ctrl+enter", 0x01, 0x28),
            ("ctrl+tab", 0x01, 0x2b),
            ("ctrl+shift+tab", 0x03, 0x2b),
        ] {
            assert_eq!(find(token), Some((token, modifier, usage)));
        }
    }

    #[test]
    fn rejects_out_of_scope_and_non_canonical_chords() {
        for token in [
            "ctrl+",
            "ctrl+1",
            "ctrl+aa",
            "CTRL+S",
            "ctrl+shift+a",
            "shift+a",
            "shift+f0",
            "shift+f13",
            "alt+s",
            "ctrl+alt+delete",
            "ctrl+shift+enter",
            "shift+ctrl+tab",
            "win+l",
            "ctrl+esc",
        ] {
            assert_eq!(find(token), None, "{token}");
        }
    }
}
