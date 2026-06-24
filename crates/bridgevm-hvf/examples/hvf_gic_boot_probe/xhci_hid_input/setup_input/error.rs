use super::super::marker::MarkerEnvError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XhciSetupInputEnvError {
    Empty,
    TooLong { len: usize, max: usize },
    TooManyActions { requested: usize, max: usize },
    UnsupportedUsage { token: String },
    UnsupportedToken { token: String },
    ArbitraryText { token: String },
    Modifier { token: String },
    Repeat { token: String },
    RamfbDelayEmpty,
    RamfbDelayTooLong { len: usize, max: usize },
    RamfbDelayTooMany { requested: usize, max: usize },
    RamfbDelayInvalid { token: String },
    RamfbDelayTooLarge { requested_ms: u64, max_ms: u64 },
    RamfbDelayDuplicate { delay_ms: u64 },
    Marker(MarkerEnvError),
}

impl XhciSetupInputEnvError {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong { .. } => "too_long",
            Self::TooManyActions { .. } => "too_many_actions",
            Self::UnsupportedUsage { .. } => "unsupported_usage",
            Self::UnsupportedToken { .. } => "unsupported_token",
            Self::ArbitraryText { .. } => "arbitrary_text",
            Self::Modifier { .. } => "modifier",
            Self::Repeat { .. } => "repeat",
            Self::RamfbDelayEmpty => "ramfb_delay_empty",
            Self::RamfbDelayTooLong { .. } => "ramfb_delay_too_long",
            Self::RamfbDelayTooMany { .. } => "ramfb_delay_too_many",
            Self::RamfbDelayInvalid { .. } => "ramfb_delay_invalid",
            Self::RamfbDelayTooLarge { .. } => "ramfb_delay_too_large",
            Self::RamfbDelayDuplicate { .. } => "ramfb_delay_duplicate",
            Self::Marker(error) => error.name(),
        }
    }
}

pub(crate) fn print_setup_input_rejection(name: &'static str, error: &XhciSetupInputEnvError) {
    if let XhciSetupInputEnvError::Marker(marker_error) = error {
        println!(
            "xHCI setup-input injection {name} rejected: parse_error={} {} queued_actions=0 queued_reports=0 emitted_key_reports=0 emitted_release_reports=0 rejected_count=1 ramfb_marker_intent=observe-only",
            error.name(),
            marker_error.rejection_summary()
        );
        return;
    }
    println!(
        "xHCI setup-input injection {name} rejected: parse_error={} queued_actions=0 queued_reports=0 emitted_key_reports=0 emitted_release_reports=0 rejected_count=1 ramfb_marker_intent=observe-only",
        error.name()
    );
}
