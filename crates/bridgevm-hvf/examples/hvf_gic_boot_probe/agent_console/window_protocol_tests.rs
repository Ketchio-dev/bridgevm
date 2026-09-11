use super::*;

#[test]
fn winlist_stays_in_flight_until_terminator() {
    let title = base64_encode("Untitled - Notepad".as_bytes());
    let record = format!("WIN 42 7 50 60 700 500 {title}");
    assert!(matches!(handle_window_reply(&record, "WINLIST"), Some(ReplyProgress::Incomplete)));
    assert!(matches!(handle_window_reply("WINEND", "WINLIST"), Some(ReplyProgress::Complete)));
    assert!(matches!(handle_window_reply("WIN nope", "WINLIST"), Some(ReplyProgress::Incomplete)));
}

#[test]
fn mutations_require_the_matching_reply() {
    for verb in ["WINBOUNDS", "WINFOCUS", "WINCLOSE"] {
        assert!(matches!(
            handle_window_reply(&format!("OK {verb}"), verb),
            Some(ReplyProgress::Complete)
        ));
    }
    assert!(matches!(
        handle_window_reply("OK WINFOCUS", "WINCLOSE"),
        Some(ReplyProgress::Ignored)
    ));
}

#[test]
fn correlated_query_labels_receipts_but_preserves_guest_wire_grammar() {
    let id = "12345678-1234-1234-1234-123456789abc";
    let command = format!("WINLIST {id}");
    assert_eq!(window_request_id(&command), Some(id));
    assert_eq!(window_list_label(&command), command);
    assert_eq!(command_wire_line(&command), "WINLIST\n");
    assert_eq!(window_list_label("WINLIST"), "WINLIST");
    assert_eq!(command_wire_line("WINLIST"), "WINLIST\n");
}

#[test]
fn malformed_or_injected_query_ids_are_not_correlated() {
    for command in [
        "WINLIST short", "WINLIST 12345678-1234-1234-1234-123456789abg",
        "WINLIST 12345678-1234-1234-1234-123456789abc\nWINCLOSE 1",
        "WINLIST  12345678-1234-1234-1234-123456789abc",
        "WINFOCUS 12345678-1234-1234-1234-123456789abc",
    ] {
        assert_eq!(window_request_id(command), None);
    }
}
