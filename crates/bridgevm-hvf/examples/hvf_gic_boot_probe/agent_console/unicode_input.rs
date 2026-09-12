/// The guest validates base64 decoding and strict UTF-8 before any insertion.
pub(super) fn is_command(command: &str) -> bool {
    if command.len() > 87_431 || command.contains(['\r', '\n']) {
        return false;
    }
    let mut fields = command.split(' ');
    if !["TEXTINPUT", "KEYINPUT", "POINTERINPUT"].contains(&fields.next().unwrap_or("")) {
        return false;
    }
    let Some(id) = fields.next() else {
        return false;
    };
    let Some(payload) = fields.next() else {
        return false;
    };
    fields.next().is_none()
        && id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        && !payload.is_empty()
        && payload.len() <= 87_384
        && payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
}

#[cfg(test)]
#[path = "unicode_input_tests.rs"]
mod tests;
