use super::super::clip_paste::LEGACY_PASTE_MARKER;

/// Input payloads belong on the wire, never in diagnostic command labels.
pub(super) fn label(command: &str) -> &str {
    if let Some(label) = legacy_paste_label(command) {
        return label;
    }
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    if !matches!(verb, "TEXTINPUT" | "KEYINPUT" | "POINTERINPUT") {
        return command;
    }
    let id = rest.split_once(' ').map_or(rest, |(id, _)| id);
    if valid_request_id(id) {
        &command[..verb.len() + 1 + id.len()]
    } else {
        "INPUT <redacted>"
    }
}

fn legacy_paste_label(command: &str) -> Option<&str> {
    let body = command.strip_prefix("powershell -NoProfile -STA -Command \"")?;
    let (_, output) = body.rsplit_once(";Write-Output '")?;
    let label = output.strip_suffix("'\"")?;
    valid_request_id(label.strip_prefix(LEGACY_PASTE_MARKER)?).then_some(label)
}

fn valid_request_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
#[path = "input_receipt_label_tests.rs"]
mod tests;
