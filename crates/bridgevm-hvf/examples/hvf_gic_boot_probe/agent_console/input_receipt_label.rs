/// Input payloads belong on the wire, never in diagnostic command labels.
pub(super) fn label(command: &str) -> &str {
    let (verb, rest) = command.split_once(' ').unwrap_or((command, ""));
    if !matches!(verb, "TEXTINPUT" | "KEYINPUT") {
        return command;
    }
    let id = rest.split_once(' ').map_or(rest, |(id, _)| id);
    let valid = id.len() == 36 && id.bytes().enumerate().all(|(i, b)| {
        if matches!(i, 8 | 13 | 18 | 23) { b == b'-' } else { b.is_ascii_hexdigit() }
    });
    if valid { &command[..verb.len() + 1 + id.len()] } else { "INPUT <redacted>" }
}

#[cfg(test)]
mod tests {
    use super::label;
    const ID: &str = "01234567-89AB-CDEF-0123-456789ABCDEF";

    #[test]
    fn input_receipt_labels_never_repeat_payloads() {
        for verb in ["TEXTINPUT", "KEYINPUT"] {
            for payload in ["cHJpdmF0ZQ==", "malformed-private", "", "line\nbreak"] {
                let command = format!("{verb} {ID} {payload}");
                assert_eq!(label(&command), format!("{verb} {ID}"));
            }
        }
    }

    #[test]
    fn input_receipt_labels_are_idempotent() {
        let command = format!("TEXTINPUT {ID} cHJpdmF0ZQ==");
        assert_eq!(label(label(&command)), label(&command));
    }

    #[test]
    fn malformed_input_identity_is_not_logged() {
        for command in ["TEXTINPUT", "KEYINPUT private", "TEXTINPUT invalid secret",
                        "TEXTINPUT  secret", "KEYINPUT bad\tid secret"] {
            assert_eq!(label(command), "INPUT <redacted>");
        }
    }

    #[test]
    fn unrelated_command_labels_are_unchanged() {
        for command in ["PING", "INPUTCAPS id", "RUN YQ==", "CLIPGET"] {
            assert_eq!(label(command), command);
        }
    }
}
