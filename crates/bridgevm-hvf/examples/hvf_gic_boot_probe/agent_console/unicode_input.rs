/// The guest validates base64 decoding and strict UTF-8 before any insertion.
pub(super) fn is_command(command: &str) -> bool {
    if command.len() > 87_431 || command.contains(['\r', '\n']) {
        return false;
    }
    let mut fields = command.split(' ');
    if !matches!(fields.next(), Some("TEXTINPUT") | Some("KEYINPUT")) {
        return false;
    }
    let Some(id) = fields.next() else { return false };
    let Some(payload) = fields.next() else { return false };
    fields.next().is_none()
        && id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) { b == b'-' } else { b.is_ascii_hexdigit() }
        })
        && !payload.is_empty()
        && payload.len() <= 87_384
        && payload.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
}

#[cfg(test)]
mod tests {
    use super::is_command;
    const ID: &str = "01234567-89AB-CDEF-0123-456789ABCDEF";

    #[test]
    fn recognizes_single_correlated_unicode_request() {
        assert!(is_command(&format!("TEXTINPUT {ID} 7ZWc")));
        assert!(is_command(&format!("TEXTINPUT {ID} YQ==")));
        assert!(is_command(&format!("KEYINPUT {ID} ZW50ZXI=")));
        assert!(!is_command(&format!("RUN {ID} YQ==")));
    }

    #[test]
    fn rejects_bad_identity_extra_fields_and_multiline_commands() {
        for value in ["TEXTINPUT invalid YQ==".to_owned(), format!("TEXTINPUT {ID} "),
            format!("TEXTINPUT {ID} YQ== extra"), format!("TEXTINPUT {ID} YQ==\nPING"),
            format!("TEXTINPUT {ID} YQ==\r"), format!("TEXTINPUT {ID} !")] {
            assert!(!is_command(&value), "{value}");
        }
    }

    #[test]
    fn bounds_encoded_input_before_parsing() {
        assert!(is_command(&format!("TEXTINPUT {ID} {}", "A".repeat(87_384))));
        assert!(!is_command(&format!("TEXTINPUT {ID} {}", "A".repeat(87_385))));
    }
}
