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
        .split([',', ' ', '\t', '\n'])
        .filter(|token| !token.is_empty())
    {
        actions.push(parse_setup_input_action(token)?);
        if actions.len() > SETUP_INPUT_MAX_ACTIONS {
            return Err(XhciSetupInputEnvError::TooManyActions {
                requested: actions.len(),
                max: SETUP_INPUT_MAX_ACTIONS,
            });
        }
    }
    if actions.is_empty() {
        return Err(XhciSetupInputEnvError::Empty);
    }
    Ok(actions)
}

fn parse_setup_input_action(token: &str) -> Result<SetupInputAction, XhciSetupInputEnvError> {
    let normalized = token.to_ascii_lowercase();
    if normalized.contains('+') {
        return Err(XhciSetupInputEnvError::Modifier {
            token: token.to_string(),
        });
    }
    if normalized.contains('*') {
        return Err(XhciSetupInputEnvError::Repeat {
            token: token.to_string(),
        });
    }
    if normalized.starts_with("0x") || normalized.starts_with("usage:") {
        return Err(XhciSetupInputEnvError::UnsupportedUsage {
            token: token.to_string(),
        });
    }
    match normalized.as_str() {
        "tab" => Ok(SetupInputAction::Tab),
        "enter" => Ok(SetupInputAction::Enter),
        "space" => Ok(SetupInputAction::Space),
        "esc" | "escape" | "left" | "right" | "up" | "down" => {
            Err(XhciSetupInputEnvError::UnsupportedToken {
                token: token.to_string(),
            })
        }
        _ => Err(XhciSetupInputEnvError::ArbitraryText {
            token: token.to_string(),
        }),
    }
}
