//! Agent console environment and command parsing.

use super::*;

pub(super) fn agent_commands_from_env() -> Vec<String> {
    let value = std::env::var(CMDS_ENV).unwrap_or_else(|_| DEFAULT_CMDS.to_string());
    value
        .split('|')
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

pub(super) fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}
