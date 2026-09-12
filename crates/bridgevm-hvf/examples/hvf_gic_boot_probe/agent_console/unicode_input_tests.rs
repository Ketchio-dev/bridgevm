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
    for value in [
        "TEXTINPUT invalid YQ==".to_owned(),
        format!("TEXTINPUT {ID} "),
        format!("TEXTINPUT {ID} YQ== extra"),
        format!("TEXTINPUT {ID} YQ==\nPING"),
        format!("TEXTINPUT {ID} YQ==\r"),
        format!("TEXTINPUT {ID} !"),
    ] {
        assert!(!is_command(&value), "{value}");
    }
}

#[test]
fn bounds_encoded_input_before_parsing() {
    assert!(is_command(&format!(
        "TEXTINPUT {ID} {}",
        "A".repeat(87_384)
    )));
    assert!(!is_command(&format!(
        "TEXTINPUT {ID} {}",
        "A".repeat(87_385)
    )));
}

#[test]
fn pointer_wire_requests_are_correlated_bounded_and_routed() {
    let command = format!("POINTERINPUT {ID} Y2xpY2s6MHgw");
    assert!(is_command(&command));
    assert!(super::super::is_command(&command));
    for command in [
        "POINTERINPUT bad YQ==".to_owned(),
        format!("POINTERINPUT {ID} "),
        format!("POINTERINPUT {ID} YQ== extra"),
        format!("POINTERINPUT {ID} YQ==\r"),
        format!("POINTERINPUT {ID} YQ==\nPING"),
        format!("POINTERINPUT {ID} !"),
        format!("POINTERINPUT {ID} {}", "A".repeat(87_385)),
    ] {
        assert!(!is_command(&command), "invalid pointer framing");
    }
}
