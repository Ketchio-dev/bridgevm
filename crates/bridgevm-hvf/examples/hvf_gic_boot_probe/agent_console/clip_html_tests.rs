use super::*;

/// The header's value for `name`, and the slice between two offsets.
fn field(cf: &[u8], name: &str) -> usize {
    let text = std::str::from_utf8(cf).unwrap();
    let line = text.lines().find(|line| line.starts_with(name)).unwrap();
    line[name.len() + 1..].parse().unwrap()
}

fn slice<'a>(cf: &'a [u8], start: &str, end: &str) -> &'a str {
    std::str::from_utf8(&cf[field(cf, start)..field(cf, end)]).unwrap()
}

#[test]
fn a_mac_fragment_is_wrapped_and_its_offsets_are_utf8_bytes() {
    let cf = cf_html_from_mac("<meta charset='utf-8'><b>굵게</b> 🙂");
    assert!(cf.starts_with(b"Version:0.9\r\nStartHTML:0000000105\r\n"));
    assert_eq!(slice(&cf, "StartFragment", "EndFragment"), "<meta charset='utf-8'><b>굵게</b> 🙂");
    let doc = slice(&cf, "StartHTML", "EndHTML");
    assert!(doc.starts_with("<html><body>") && doc.ends_with("</html>"), "{doc}");
    assert_eq!(field(&cf, "EndHTML"), cf.len());
}

#[test]
fn a_mac_document_keeps_its_head_and_marks_its_body() {
    let html = "<html><head><style>p{color:red}</style></head><BODY class=x>\n<p>한글</p>\n</body></html>";
    let cf = cf_html_from_mac(html);
    assert_eq!(slice(&cf, "StartFragment", "EndFragment"), "\n<p>한글</p>\n");
    assert_eq!(
        slice(&cf, "StartHTML", "EndHTML"),
        "<html><head><style>p{color:red}</style></head><BODY class=x><!--StartFragment-->\n<p>한글</p>\n<!--EndFragment--></body></html>"
    );
}

#[test]
fn markers_already_in_the_html_are_kept() {
    let html = "<html><body><!--StartFragment--><i>x</i><!--EndFragment--></body></html>";
    let cf = cf_html_from_mac(html);
    assert_eq!(slice(&cf, "StartHTML", "EndHTML"), html);
    assert_eq!(slice(&cf, "StartFragment", "EndFragment"), "<i>x</i>");
}

#[test]
fn a_bom_and_nul_bytes_never_reach_the_guest() {
    let cf = cf_html_from_mac("\u{feff}<b>a\0b</b>");
    assert!(!cf.contains(&0));
    assert_eq!(slice(&cf, "StartFragment", "EndFragment"), "<b>ab</b>");
}

#[test]
fn a_guest_document_reaches_the_mac_whole_and_marked_utf8() {
    let cf = cf_html_from_mac("<html><head><style>b{}</style></head><body><b>굵게</b></body></html>");
    let mac = mac_html_from_cf(&cf).unwrap();
    assert_eq!(
        mac,
        "<meta charset=\"utf-8\"><html><head><style>b{}</style></head><body><!--StartFragment--><b>굵게</b><!--EndFragment--></body></html>"
    );
}

#[test]
fn chrome_and_word_style_headers_parse() {
    // Chrome: Version 0.9, no SourceURL; Word: Version 1.0 with SourceURL and -1 offsets allowed.
    let doc = "<html><body>\r\n<!--StartFragment--><p>x</p><!--EndFragment-->\r\n</body></html>";
    let head = "Version:1.0\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\nSourceURL:file:///C:/a.docx\r\n";
    let fragment_start = head.len() + doc.find("<p>").unwrap();
    let fragment_end = head.len() + doc.find("<!--End").unwrap();
    let head = head
        .replacen("StartFragment:0000000000", &format!("StartFragment:{fragment_start:010}"), 1)
        .replacen("EndFragment:0000000000", &format!("EndFragment:{fragment_end:010}"), 1);
    let cf = format!("{head}{doc}");
    assert_eq!(mac_html_from_cf(cf.as_bytes()).unwrap(), "<meta charset=\"utf-8\"><p>x</p>");
    // An EndHTML that counted the NUL the agent cut is clamped.
    let cf = cf_html_from_mac("<i>y</i>");
    let text = String::from_utf8(cf.clone()).unwrap();
    let end = field(&cf, "EndHTML");
    let past = text.replacen(&format!("EndHTML:{end:010}"), &format!("EndHTML:{:010}", end + 1), 1);
    assert!(mac_html_from_cf(past.as_bytes()).unwrap().contains("<i>y</i>"));
}

#[test]
fn unusable_guest_html_is_refused() {
    assert_eq!(mac_html_from_cf(b"\xff\xfe<b>x</b>"), None, "not UTF-8");
    assert_eq!(mac_html_from_cf(b"<b>no header</b>"), None, "no offsets");
    let cf = "Version:0.9\r\nStartHTML:0000000105\r\nEndHTML:0000000107\r\nStartFragment:0000000105\r\nEndFragment:0000000107\r\n가";
    assert_eq!(mac_html_from_cf(cf.as_bytes()), None, "an offset inside a character");
    let cf = "Version:0.9\r\nStartHTML:0000000900\r\nEndHTML:0000000999\r\n<b>x</b>";
    assert_eq!(mac_html_from_cf(cf.as_bytes()), None, "offsets past the end");
}
