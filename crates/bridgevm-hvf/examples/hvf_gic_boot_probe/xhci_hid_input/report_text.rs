use bridgevm_hvf::xhci::{SetupInputAction, XhciSetupInputQueueError};

pub(crate) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[derive(Debug, Default)]
pub(crate) struct IncrementalMarkerScan {
    scanned_len: usize,
    found: bool,
}

impl IncrementalMarkerScan {
    pub(crate) fn contains_new(&mut self, haystack: &[u8], needle: &[u8]) -> bool {
        if self.found {
            return true;
        }
        if needle.is_empty() {
            self.found = true;
            return true;
        }
        let overlap = needle.len().saturating_sub(1);
        let start = self.scanned_len.saturating_sub(overlap).min(haystack.len());
        self.scanned_len = haystack.len();
        self.found = contains_bytes(&haystack[start..], needle);
        self.found
    }
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
