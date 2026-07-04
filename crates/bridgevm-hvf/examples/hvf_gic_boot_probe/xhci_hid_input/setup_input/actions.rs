use bridgevm_hvf::xhci::SetupInputAction;

use super::{XhciSetupInputEnvError, SETUP_INPUT_ENV_MAX_BYTES, SETUP_INPUT_MAX_ACTIONS};

pub(super) fn parse_setup_input_actions(
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
        value if value.contains('+') => {
            return Err(XhciSetupInputEnvError::Modifier {
                token: token.to_string(),
            });
        }
        "esc" | "escape" | "left" | "right" | "up" | "down" => {
            return Err(XhciSetupInputEnvError::UnsupportedToken {
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
    let (name, usage) = match ch {
        'a' => ("a", 0x04),
        'b' => ("b", 0x05),
        'c' => ("c", 0x06),
        'd' => ("d", 0x07),
        'e' => ("e", 0x08),
        'f' => ("f", 0x09),
        'g' => ("g", 0x0a),
        'h' => ("h", 0x0b),
        'i' => ("i", 0x0c),
        'j' => ("j", 0x0d),
        'k' => ("k", 0x0e),
        'l' => ("l", 0x0f),
        'm' => ("m", 0x10),
        'n' => ("n", 0x11),
        'o' => ("o", 0x12),
        'p' => ("p", 0x13),
        'q' => ("q", 0x14),
        'r' => ("r", 0x15),
        's' => ("s", 0x16),
        't' => ("t", 0x17),
        'u' => ("u", 0x18),
        'v' => ("v", 0x19),
        'w' => ("w", 0x1a),
        'x' => ("x", 0x1b),
        'y' => ("y", 0x1c),
        'z' => ("z", 0x1d),
        '1' => ("1", 0x1e),
        '2' => ("2", 0x1f),
        '3' => ("3", 0x20),
        '4' => ("4", 0x21),
        '5' => ("5", 0x22),
        '6' => ("6", 0x23),
        '7' => ("7", 0x24),
        '8' => ("8", 0x25),
        '9' => ("9", 0x26),
        '0' => ("0", 0x27),
        '/' => ("/", 0x38),
        '-' => ("-", 0x2d),
        '.' => (".", 0x37),
        _ => return None,
    };
    Some(SetupInputAction::Key {
        name,
        modifier: 0,
        usage,
    })
}
