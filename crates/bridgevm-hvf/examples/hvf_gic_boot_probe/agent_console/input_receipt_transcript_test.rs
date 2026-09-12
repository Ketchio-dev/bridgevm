#[test]
fn input_receipt_cross_language_transcript() {
    let id = "01234567-89AB-CDEF-0123-456789ABCDEF";
    for (name, verb, value, mode) in [
        ("normal", "TEXTINPUT", "private-input-fixture", 0),
        ("chunked", "TEXTINPUT", "private-input-fixture", 1),
        ("key", "KEYINPUT", "ctrl+v", 0),
        ("missing-begin", "TEXTINPUT", "private-input-fixture", 2),
    ] {
        let mut h = harness();
        let command = format!("{verb} {id} {}", base64_encode(value.as_bytes()));
        let count = if verb == "KEYINPUT" { 4 } else { value.encode_utf16().count() * 2 };
        let marker = format!("BVINPUT_INSERTED {id} {count}");
        println!("\nBEGIN_BV_INPUT_FIXTURE {name}");
        let terminal = match mode {
            0 => h.handle_reply_line(&format!("OUT 0 {}", base64_encode(marker.as_bytes())), &command),
            1 => {
                assert!(matches!(h.handle_reply_line(
                    &format!("OUTBEG 0 {} 1", marker.len()), &command
                ), ReplyProgress::Incomplete));
                assert!(matches!(h.handle_reply_line(
                    &format!("OUTCHUNK 0 {}", base64_encode(marker.as_bytes())), &command
                ), ReplyProgress::Incomplete));
                h.handle_reply_line("OUTEND 1", &command)
            }
            _ => h.handle_reply_line("OUTEND 1", &command),
        };
        assert!(matches!(terminal, ReplyProgress::Complete));
        println!("END_BV_INPUT_FIXTURE {name}");
    }
}
