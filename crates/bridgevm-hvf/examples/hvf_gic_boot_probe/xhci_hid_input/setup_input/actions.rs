use bridgevm_hvf::xhci::SetupInputAction;

use super::{XhciSetupInputEnvError, SETUP_INPUT_ENV_MAX_BYTES, SETUP_INPUT_MAX_ACTIONS};

pub(crate) fn parse_setup_input_actions(
    value: &str,
) -> Result<Vec<SetupInputAction>, XhciSetupInputEnvError> {
    if value.len() > SETUP_INPUT_ENV_MAX_BYTES {
        return Err(XhciSetupInputEnvError::TooLong {
            len: value.len(),
            max: SETUP_INPUT_ENV_MAX_BYTES,
        });
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(XhciSetupInputEnvError::Empty);
    }
    let mut actions = Vec::new();
    for token in trimmed
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        parse_setup_input_token(token, &mut actions)?;
    }
    if actions.is_empty() {
        return Err(XhciSetupInputEnvError::Empty);
    }
    Ok(actions)
}

fn parse_setup_input_token(
    token: &str,
    actions: &mut Vec<SetupInputAction>,
) -> Result<(), XhciSetupInputEnvError> {
    let normalized = token.to_ascii_lowercase();
    if token.contains('*') {
        return Err(XhciSetupInputEnvError::Repeat {
            token: token.to_string(),
        });
    }
    if let Some(text) = token.strip_prefix("text:") {
        if text.is_empty() {
            return Err(XhciSetupInputEnvError::UnsupportedToken {
                token: token.to_string(),
            });
        }
        for ch in text.chars() {
            let Some(action) = ascii_text_action(ch) else {
                return Err(XhciSetupInputEnvError::UnsupportedToken {
                    token: token.to_string(),
                });
            };
            push_setup_input_action(actions, action)?;
        }
        return Ok(());
    }
    if let Some(hex) = token.strip_prefix("text-hex:") {
        if hex.is_empty() || hex.len() % 2 != 0 {
            return Err(XhciSetupInputEnvError::UnsupportedToken {
                token: token.to_string(),
            });
        }
        for pair in hex.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair).map_err(|_| {
                XhciSetupInputEnvError::UnsupportedToken {
                    token: token.to_string(),
                }
            })?;
            let byte = u8::from_str_radix(pair, 16).map_err(|_| {
                XhciSetupInputEnvError::UnsupportedToken {
                    token: token.to_string(),
                }
            })?;
            let Some(action) = ascii_text_action(char::from(byte)) else {
                return Err(XhciSetupInputEnvError::UnsupportedToken {
                    token: token.to_string(),
                });
            };
            push_setup_input_action(actions, action)?;
        }
        return Ok(());
    }
    if normalized.starts_with("0x") || normalized.starts_with("usage:") {
        return Err(XhciSetupInputEnvError::UnsupportedUsage {
            token: token.to_string(),
        });
    }
    let action = match normalized.as_str() {
        "tab" => SetupInputAction::Tab,
        "enter" => SetupInputAction::Enter,
        "space" => SetupInputAction::Space,
        "win+r" | "lgui+r" => SetupInputAction::Key {
            name: "win+r",
            modifier: 0x08,
            usage: 0x15,
        },
        "ctrl+alt+delete" | "ctrl+alt+del" => SetupInputAction::Key {
            name: "ctrl+alt+delete",
            modifier: 0x05,
            usage: 0x4c,
        },
        "esc" | "escape" => SetupInputAction::Key {
            name: "esc",
            modifier: 0,
            usage: 0x29,
        },
        "backspace" => SetupInputAction::Key {
            name: "backspace",
            modifier: 0,
            usage: 0x2a,
        },
        "delete" | "del" => SetupInputAction::Key {
            name: "delete",
            modifier: 0,
            usage: 0x4c,
        },
        "f1" => SetupInputAction::Key {
            name: "f1",
            modifier: 0,
            usage: 0x3a,
        },
        "f2" => SetupInputAction::Key {
            name: "f2",
            modifier: 0,
            usage: 0x3b,
        },
        "f3" => SetupInputAction::Key {
            name: "f3",
            modifier: 0,
            usage: 0x3c,
        },
        "f4" => SetupInputAction::Key {
            name: "f4",
            modifier: 0,
            usage: 0x3d,
        },
        "f5" => SetupInputAction::Key {
            name: "f5",
            modifier: 0,
            usage: 0x3e,
        },
        "f6" => SetupInputAction::Key {
            name: "f6",
            modifier: 0,
            usage: 0x3f,
        },
        "f7" => SetupInputAction::Key {
            name: "f7",
            modifier: 0,
            usage: 0x40,
        },
        "f8" => SetupInputAction::Key {
            name: "f8",
            modifier: 0,
            usage: 0x41,
        },
        "f9" => SetupInputAction::Key {
            name: "f9",
            modifier: 0,
            usage: 0x42,
        },
        "f10" => SetupInputAction::Key {
            name: "f10",
            modifier: 0,
            usage: 0x43,
        },
        "f11" => SetupInputAction::Key {
            name: "f11",
            modifier: 0,
            usage: 0x44,
        },
        "f12" => SetupInputAction::Key {
            name: "f12",
            modifier: 0,
            usage: 0x45,
        },
        "right" => SetupInputAction::Key {
            name: "right",
            modifier: 0,
            usage: 0x4f,
        },
        "left" => SetupInputAction::Key {
            name: "left",
            modifier: 0,
            usage: 0x50,
        },
        "down" => SetupInputAction::Key {
            name: "down",
            modifier: 0,
            usage: 0x51,
        },
        "up" => SetupInputAction::Key {
            name: "up",
            modifier: 0,
            usage: 0x52,
        },
        "home" => SetupInputAction::Key {
            name: "home",
            modifier: 0,
            usage: 0x4a,
        },
        "end" => SetupInputAction::Key {
            name: "end",
            modifier: 0,
            usage: 0x4d,
        },
        "pageup" => SetupInputAction::Key {
            name: "pageup",
            modifier: 0,
            usage: 0x4b,
        },
        "pagedown" => SetupInputAction::Key {
            name: "pagedown",
            modifier: 0,
            usage: 0x4e,
        },
        value if value.contains('+') => {
            return Err(XhciSetupInputEnvError::Modifier {
                token: token.to_string(),
            });
        }
        _ => {
            return Err(XhciSetupInputEnvError::ArbitraryText {
                token: token.to_string(),
            });
        }
    };
    push_setup_input_action(actions, action)
}

fn push_setup_input_action(
    actions: &mut Vec<SetupInputAction>,
    action: SetupInputAction,
) -> Result<(), XhciSetupInputEnvError> {
    actions.push(action);
    if actions.len() > SETUP_INPUT_MAX_ACTIONS {
        return Err(XhciSetupInputEnvError::TooManyActions {
            requested: actions.len(),
            max: SETUP_INPUT_MAX_ACTIONS,
        });
    }
    Ok(())
}

fn ascii_text_action(ch: char) -> Option<SetupInputAction> {
    let (name, modifier, usage) = match ch {
        'a' => ("a", 0, 0x04),
        'b' => ("b", 0, 0x05),
        'c' => ("c", 0, 0x06),
        'd' => ("d", 0, 0x07),
        'e' => ("e", 0, 0x08),
        'f' => ("f", 0, 0x09),
        'g' => ("g", 0, 0x0a),
        'h' => ("h", 0, 0x0b),
        'i' => ("i", 0, 0x0c),
        'j' => ("j", 0, 0x0d),
        'k' => ("k", 0, 0x0e),
        'l' => ("l", 0, 0x0f),
        'm' => ("m", 0, 0x10),
        'n' => ("n", 0, 0x11),
        'o' => ("o", 0, 0x12),
        'p' => ("p", 0, 0x13),
        'q' => ("q", 0, 0x14),
        'r' => ("r", 0, 0x15),
        's' => ("s", 0, 0x16),
        't' => ("t", 0, 0x17),
        'u' => ("u", 0, 0x18),
        'v' => ("v", 0, 0x19),
        'w' => ("w", 0, 0x1a),
        'x' => ("x", 0, 0x1b),
        'y' => ("y", 0, 0x1c),
        'z' => ("z", 0, 0x1d),
        'A' => ("A", 0x02, 0x04),
        'B' => ("B", 0x02, 0x05),
        'C' => ("C", 0x02, 0x06),
        'D' => ("D", 0x02, 0x07),
        'E' => ("E", 0x02, 0x08),
        'F' => ("F", 0x02, 0x09),
        'G' => ("G", 0x02, 0x0a),
        'H' => ("H", 0x02, 0x0b),
        'I' => ("I", 0x02, 0x0c),
        'J' => ("J", 0x02, 0x0d),
        'K' => ("K", 0x02, 0x0e),
        'L' => ("L", 0x02, 0x0f),
        'M' => ("M", 0x02, 0x10),
        'N' => ("N", 0x02, 0x11),
        'O' => ("O", 0x02, 0x12),
        'P' => ("P", 0x02, 0x13),
        'Q' => ("Q", 0x02, 0x14),
        'R' => ("R", 0x02, 0x15),
        'S' => ("S", 0x02, 0x16),
        'T' => ("T", 0x02, 0x17),
        'U' => ("U", 0x02, 0x18),
        'V' => ("V", 0x02, 0x19),
        'W' => ("W", 0x02, 0x1a),
        'X' => ("X", 0x02, 0x1b),
        'Y' => ("Y", 0x02, 0x1c),
        'Z' => ("Z", 0x02, 0x1d),
        '1' => ("1", 0, 0x1e),
        '2' => ("2", 0, 0x1f),
        '3' => ("3", 0, 0x20),
        '4' => ("4", 0, 0x21),
        '5' => ("5", 0, 0x22),
        '6' => ("6", 0, 0x23),
        '7' => ("7", 0, 0x24),
        '8' => ("8", 0, 0x25),
        '9' => ("9", 0, 0x26),
        '0' => ("0", 0, 0x27),
        '!' => ("!", 0x02, 0x1e),
        '@' => ("@", 0x02, 0x1f),
        '#' => ("#", 0x02, 0x20),
        '$' => ("$", 0x02, 0x21),
        '%' => ("%", 0x02, 0x22),
        '^' => ("^", 0x02, 0x23),
        '&' => ("&", 0x02, 0x24),
        '*' => ("*", 0x02, 0x25),
        '(' => ("(", 0x02, 0x26),
        ')' => (")", 0x02, 0x27),
        ' ' => ("space", 0, 0x2c),
        '-' => ("-", 0, 0x2d),
        '_' => ("_", 0x02, 0x2d),
        '=' => ("=", 0, 0x2e),
        '+' => ("+", 0x02, 0x2e),
        '[' => ("[", 0, 0x2f),
        '{' => ("{", 0x02, 0x2f),
        ']' => ("]", 0, 0x30),
        '}' => ("}", 0x02, 0x30),
        '\\' => ("\\", 0, 0x31),
        '|' => ("|", 0x02, 0x31),
        ';' => (";", 0, 0x33),
        ':' => (":", 0x02, 0x33),
        '\'' => ("'", 0, 0x34),
        '"' => ("\"", 0x02, 0x34),
        '`' => ("`", 0, 0x35),
        '~' => ("~", 0x02, 0x35),
        ',' => (",", 0, 0x36),
        '<' => ("<", 0x02, 0x36),
        '.' => (".", 0, 0x37),
        '>' => (">", 0x02, 0x37),
        '/' => ("/", 0, 0x38),
        '?' => ("?", 0x02, 0x38),
        _ => return None,
    };
    Some(SetupInputAction::Key {
        name,
        modifier,
        usage,
    })
}

#[cfg(test)]
mod tests {
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
}
