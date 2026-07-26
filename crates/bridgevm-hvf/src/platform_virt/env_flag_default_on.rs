//! Default-ON environment flags.
//!
//! `env_flag` (see `env_config.rs`) is opt-in: absent means off. Once a
//! setting becomes the shipping default that polarity inverts -- absent must
//! mean ON, and the variable exists only as an escape hatch. Keeping the two
//! policies as separate functions makes it impossible to flip a flag's
//! default by accident.

/// ON unless explicitly disabled.
pub(crate) fn env_flag_default_on(name: &str) -> bool {
    env_value_default_on(std::env::var(name).ok().as_deref())
}

/// Pure core of [`env_flag_default_on`], so the policy is testable without
/// mutating process-wide environment state.
pub(crate) fn env_value_default_on(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let trimmed = value.trim();
    // Exported-but-empty means "not configured", not "off".
    if trimmed.is_empty() {
        return true;
    }
    !(trimmed == "0"
        || trimmed.eq_ignore_ascii_case("false")
        || trimmed.eq_ignore_ascii_case("no")
        || trimmed.eq_ignore_ascii_case("off"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_is_on() {
        assert!(env_value_default_on(None));
    }

    /// The escape hatch. Without a reachable "off", a shipped default is a
    /// one-way door -- which is exactly what the old runner passthrough was,
    /// since it forwarded the variable only when it equalled "1".
    #[test]
    fn zero_disables() {
        assert!(!env_value_default_on(Some("0")));
        assert!(!env_value_default_on(Some("false")));
        assert!(!env_value_default_on(Some("off")));
        assert!(!env_value_default_on(Some(" 0 ")));
    }

    #[test]
    fn explicit_on_stays_on() {
        assert!(env_value_default_on(Some("1")));
        assert!(env_value_default_on(Some("on")));
        assert!(env_value_default_on(Some("TRUE")));
    }

    #[test]
    fn empty_is_not_configured_so_still_on() {
        assert!(env_value_default_on(Some("")));
        assert!(env_value_default_on(Some("   ")));
    }

    /// An unrecognised value is not a disable request; only the explicit
    /// off-words turn a shipped default off.
    #[test]
    fn unknown_value_does_not_silently_disable() {
        assert!(env_value_default_on(Some("maybe")));
    }
}
