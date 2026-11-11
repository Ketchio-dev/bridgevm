use bridgevm_hvf::xhci::SetupInputAction;
use crate::xhci_hid_input::parse_setup_input_actions;

#[path = "key_chords.rs"]
mod key_chords;

/// One input.ctl `KEY` line: a tap (each action pressed and released), or one held key's press
/// (`down:<key>`) or release (`up:<key>`). A held key is one named key, chord or bare modifier, never
/// text; the VMM keeps it in every report until its release, and Windows repeats it.
pub(super) enum KeyCommand {
    Tap(Vec<SetupInputAction>),
    Hold { action: SetupInputAction, down: bool },
}

pub(super) fn parse_command(value: &str) -> Result<KeyCommand, &'static str> {
    let trimmed = value.trim();
    let (down, key) = match (trimmed.strip_prefix("down:"), trimmed.strip_prefix("up:")) {
        (Some(key), _) => (true, key),
        (None, Some(key)) => (false, key),
        (None, None) => return parse(value).map(KeyCommand::Tap),
    };
    if key.contains([':', ',']) {
        return Err("hold");
    }
    match parse(key)?.as_slice() {
        [action] => Ok(KeyCommand::Hold {
            action: *action,
            down,
        }),
        _ => Err("hold"),
    }
}

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
    fn a_held_key_is_one_named_key_or_chord_and_a_tap_stays_a_tap() {
        for (line, down, modifier, usage) in [
            ("down:left", true, 0x00, 0x50),
            ("up:left", false, 0x00, 0x50),
            ("down:ctrl+right", true, 0x01, 0x4f),
            ("up:shift+f10", false, 0x02, 0x43),
            ("down:down", true, 0x00, 0x51),
            ("up:up", false, 0x00, 0x52),
        ] {
            assert!(
                matches!(parse_command(line), Ok(KeyCommand::Hold { action, down: d })
                    if d == down && action.usage() == usage && matches!(action,
                        SetupInputAction::Key { modifier: m, .. } if m == modifier)),
                "{line}"
            );
        }
        // `down` and `up` alone are still the arrow keys, tapped.
        assert!(matches!(parse_command("down"), Ok(KeyCommand::Tap(a)) if a[0].usage() == 0x51));
        assert!(matches!(parse_command("up"), Ok(KeyCommand::Tap(a)) if a[0].usage() == 0x52));
        for line in [
            "down:",
            "up:",
            "down:a",
            "down:text:ab",
            "down:text-hex:61",
            "down:left,right",
            "down:up:left",
            "hold:left",
            "down:nokey",
        ] {
            assert!(parse_command(line).is_err(), "{line}");
        }
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
