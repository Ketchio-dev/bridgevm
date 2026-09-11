#[path = "unicode_input.rs"]
mod encoded_input;

pub(super) fn is_command(command: &str) -> bool {
    encoded_input::is_command(command) || is_capability_query(command)
}

fn is_capability_query(command: &str) -> bool {
    let Some(id) = command.strip_prefix("INPUTCAPS ") else {
        return false;
    };
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
mod tests {
    use super::is_command;

    #[test]
    fn capabilities_require_one_canonical_uuid() {
        let id = "12345678-1234-1234-1234-123456789abc";
        assert!(is_command(&format!("INPUTCAPS {id}")));
        for suffix in ["\n", "\r", " extra", " "] {
            assert!(!is_command(&format!("INPUTCAPS {id}{suffix}")));
        }
        for command in ["INPUTCAPS", "INPUTCAPS x", "inputcaps x", "INPUTCAPS 1234"] {
            assert!(!is_command(command));
        }
    }

    #[test]
    fn text_and_key_commands_still_use_existing_classifier() {
        let id = "12345678-1234-1234-1234-123456789abc";
        assert!(is_command(&format!("TEXTINPUT {id} YQ==")));
        assert!(is_command(&format!("KEYINPUT {id} ZW50ZXI=")));
        assert!(!is_command("RUN echo INPUTCAPS"));
    }
}
