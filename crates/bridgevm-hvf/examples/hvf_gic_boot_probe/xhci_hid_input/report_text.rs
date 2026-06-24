use bridgevm_hvf::xhci::{SetupInputAction, XhciSetupInputQueueError};

pub(crate) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(crate) fn format_report(report: [u8; 8]) -> String {
    report
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn format_action_names(actions: &[SetupInputAction]) -> String {
    actions
        .iter()
        .map(|action| action.name())
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn queue_error_name(error: XhciSetupInputQueueError) -> &'static str {
    match error {
        XhciSetupInputQueueError::EmptySequence => "empty_sequence",
        XhciSetupInputQueueError::TooManyActions { .. } => "too_many_actions",
        XhciSetupInputQueueError::Busy => "busy",
    }
}
