/// Input payloads belong on the wire, never in diagnostic command labels.
pub(super) fn label(command: &str) -> &str {
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    if !matches!(verb, "TEXTINPUT" | "KEYINPUT" | "POINTERINPUT") {
        return command;
    }
    let id = rest.split_once(' ').map_or(rest, |(id, _)| id);
    let valid = id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        });
    if valid {
        &command[..verb.len() + 1 + id.len()]
    } else {
        "INPUT <redacted>"
    }
}

#[cfg(test)]
#[path = "input_receipt_label_tests.rs"]
mod tests;
