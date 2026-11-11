//! One generic "modifiers + key" chord on the legacy route: `win`, `ctrl`, `alt` and `shift`, each
//! at most once and in that order, then one key; or a single modifier alone (a bare tap, such as the
//! Windows key opening Start). The VMM writes the boot-keyboard report itself; no guest agent is
//! involved. A character key alone or with only Shift is text, which has its own path.

/// Boot-keyboard modifier bits: left GUI (the Windows key), left Ctrl, left Alt and left Shift.
const MODIFIERS: [(&str, u8); 4] = [("win", 0x08), ("ctrl", 0x01), ("alt", 0x04), ("shift", 0x02)];
const SHIFT: u8 = 0x02;

/// HID keyboard usages of the keys that never type text.
const NAMED: &[(&str, u8)] = &[
    ("enter", 0x28),
    ("esc", 0x29),
    ("escape", 0x29),
    ("backspace", 0x2a),
    ("tab", 0x2b),
    ("space", 0x2c),
    ("printscreen", 0x46),
    ("insert", 0x49),
    ("home", 0x4a),
    ("pageup", 0x4b),
    ("delete", 0x4c),
    ("end", 0x4d),
    ("pagedown", 0x4e),
    ("right", 0x4f),
    ("left", 0x50),
    ("down", 0x51),
    ("up", 0x52),
];

/// HID usages of the punctuation keys, named by their US-layout position.
const PUNCTUATION: &[(&str, u8)] = &[
    ("minus", 0x2d),
    ("equal", 0x2e),
    ("lbracket", 0x2f),
    ("rbracket", 0x30),
    ("backslash", 0x31),
    ("semicolon", 0x33),
    ("quote", 0x34),
    ("grave", 0x35),
    ("comma", 0x36),
    ("period", 0x37),
    ("slash", 0x38),
];

/// The boot-keyboard modifier byte and key usage (0 for a bare modifier) of a canonical chord.
pub(super) fn find(chord: &str) -> Option<(u8, u8)> {
    let parts: Vec<&str> = chord.split('+').collect();
    let (key, prefix) = parts.split_last()?;
    let mut modifier = 0;
    let mut next = 0;
    for part in prefix {
        let offset = MODIFIERS[next..].iter().position(|(name, _)| name == part)?;
        modifier |= MODIFIERS[next + offset].1;
        next += offset + 1;
    }
    if prefix.is_empty() {
        if let Some((_, bit)) = MODIFIERS.iter().find(|(name, _)| name == key) {
            return Some((*bit, 0));
        }
    }
    if let Some(usage) = named(key) {
        return Some((modifier, usage));
    }
    if modifier & !SHIFT == 0 {
        return None;
    }
    character(key).map(|usage| (modifier, usage))
}

/// F1-F12 at 0x3a-0x45, then the other non-text keys.
fn named(key: &str) -> Option<u8> {
    if let Some(number) = key.strip_prefix('f') {
        return (1..=12u8)
            .find(|candidate| candidate.to_string() == number)
            .map(|candidate| 0x39 + candidate);
    }
    NAMED
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, usage)| *usage)
}

/// Letters at 0x04 (A), digits at 0x1e (1) through 0x27 (0), then the punctuation keys.
fn character(key: &str) -> Option<u8> {
    match key.as_bytes() {
        [letter @ b'a'..=b'z'] => Some(0x04 + (letter - b'a')),
        [b'0'] => Some(0x27),
        [digit @ b'1'..=b'9'] => Some(0x1e + (digit - b'1')),
        _ => PUNCTUATION
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, usage)| *usage),
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::find;
    use bridgevm_hvf::xhci::SetupInputAction;

    fn report(token: &str) -> Option<(u8, u8)> {
        match parse(token).ok()?.as_slice() {
            [SetupInputAction::Key {
                modifier, usage, ..
            }] => Some((*modifier, *usage)),
            _ => None,
        }
    }

    #[test]
    fn windows_alt_ctrl_and_shift_chords_are_one_boot_keyboard_report() {
        for (token, modifier, usage) in [
            ("win", 0x08, 0x00),
            ("alt", 0x04, 0x00),
            ("win+e", 0x08, 0x08),
            ("win+r", 0x08, 0x15),
            ("win+d", 0x08, 0x07),
            ("win+v", 0x08, 0x19),
            ("win+l", 0x08, 0x0f),
            ("win+shift+s", 0x0a, 0x16),
            ("win+ctrl+d", 0x09, 0x07),
            ("win+left", 0x08, 0x50),
            ("win+period", 0x08, 0x37),
            ("ctrl+esc", 0x01, 0x29),
            ("ctrl+shift+esc", 0x03, 0x29),
            ("alt+s", 0x04, 0x16),
            ("alt+shift+f", 0x06, 0x09),
            ("alt+space", 0x04, 0x2c),
            ("ctrl+shift+a", 0x03, 0x04),
            ("ctrl+1", 0x01, 0x1e),
            ("ctrl+9", 0x01, 0x26),
            ("ctrl+0", 0x01, 0x27),
            ("ctrl+f4", 0x01, 0x3d),
            ("ctrl+equal", 0x01, 0x2e),
            ("ctrl+minus", 0x01, 0x2d),
            ("ctrl+slash", 0x01, 0x38),
            ("ctrl+alt+shift+grave", 0x07, 0x35),
            ("win+ctrl+alt+shift+f12", 0x0f, 0x45),
            ("shift+backspace", 0x02, 0x2a),
            ("shift+esc", 0x02, 0x29),
            ("insert", 0x00, 0x49),
            ("printscreen", 0x00, 0x46),
            ("alt+printscreen", 0x04, 0x46),
            ("win+printscreen", 0x08, 0x46),
            (" WIN+E ", 0x08, 0x08),
        ] {
            assert_eq!(report(token), Some((modifier, usage)), "{token}");
        }
    }

    #[test]
    fn every_earlier_chord_keeps_its_report() {
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
            ("shift+left", 0x02, 0x50),
            ("ctrl+right", 0x01, 0x4f),
            ("ctrl+shift+home", 0x03, 0x4a),
            ("ctrl+shift+pagedown", 0x03, 0x4e),
            ("shift+tab", 0x02, 0x2b),
            ("shift+enter", 0x02, 0x28),
            ("shift+space", 0x02, 0x2c),
            ("ctrl+backspace", 0x01, 0x2a),
            ("ctrl+delete", 0x01, 0x4c),
            ("alt+tab", 0x04, 0x2b),
            ("alt+f4", 0x04, 0x3d),
            ("alt+enter", 0x04, 0x28),
            ("ctrl+alt+delete", 0x05, 0x4c),
            ("win+r", 0x08, 0x15),
            ("esc", 0x00, 0x29),
            ("f1", 0x00, 0x3a),
        ] {
            assert_eq!(report(token), Some((modifier, usage)), "{token}");
        }
        for (index, letter) in ('a'..='z').enumerate() {
            assert_eq!(
                find(&format!("ctrl+{letter}")),
                Some((0x01, 0x04 + index as u8))
            );
        }
        for number in 1..=12u8 {
            assert_eq!(find(&format!("shift+f{number}")), Some((0x02, 0x39 + number)));
        }
    }

    #[test]
    fn rejects_text_unknown_keys_and_non_canonical_chords() {
        for token in [
            "",
            "+",
            "ctrl+",
            "a",
            "1",
            "shift+a",
            "shift+1",
            "shift+slash",
            "ctrl+aa",
            "CTRL+S",
            "shift+ctrl+tab",
            "ctrl+win+e",
            "shift+alt+s",
            "ctrl+ctrl+v",
            "win+win",
            "ctrl+win",
            "command+c",
            "ctrl+f0",
            "ctrl+f01",
            "ctrl+f13",
            "shift+f13",
            "ctrl+usage:0x19",
            "ctrl+,",
            "ctrl+s,ctrl+s",
            "ctrl+v+v",
        ] {
            assert_eq!(find(token), None, "{token}");
        }
        for token in ["shift+s", "ctrl+s,ctrl+s", "shift+left,enter", "shift+f13", "win+"] {
            assert!(parse(token).is_err(), "{token}");
        }
    }
}
