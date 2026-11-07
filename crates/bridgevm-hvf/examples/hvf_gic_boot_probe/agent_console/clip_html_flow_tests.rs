//! Rich text (HTML) through the resident service loop: CLIPXH polls and
//! CLIPSETH pushes against the Rig's Mac and guest (clip_image_tests.rs).

use super::*;
use crate::agent_console::clip_html::{cf_html_from_mac, mac_html_from_cf, CLIP_HTML_MAX_BYTES};

/// A service whose agent announced CF_HTML, with its startup traffic cleared.
fn rig() -> Rig {
    let mut rig = Rig::unsettled(true);
    rig.guest.html_caps = true;
    rig.settle();
    assert!(rig.h.clip_html);
    assert!(rig.wire.iter().any(|line| line == "CLIPXH"), "polls carry the HTML");
    rig.verbs();
    rig
}

#[test]
fn a_mac_rich_copy_reaches_the_guest_in_one_write_and_never_bounces() {
    let mut rig = rig();
    rig.mac.copy_rich("bold\nline", "<meta charset='utf-8'><b>bold</b><br>line");
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPSETH"]));
    assert_eq!(rig.guest.text, "bold\nline");
    let cf = rig.guest.html.clone().expect("HTML Format");
    assert!(mac_html_from_cf(&cf).unwrap().contains("<b>bold</b><br>line"));
    let mut names = Vec::new();
    let lines = take_lines(&mut names);
    assert_eq!(lines.last().unwrap(), &format!("BVAGENT CLIPSYNC host->guest bytes=10 html={} copy=a", cf.len()));
    for _ in 0..3 {
        rig.settle();
    }
    assert!(rig.verbs().is_empty(), "the echo is held");
    assert!(rig.mac.writes.is_empty() && rig.mac.rich_writes.is_empty(), "nothing comes back to the Mac");
}

#[test]
fn a_guest_rich_copy_reaches_the_mac_with_its_html() {
    let mut rig = rig();
    rig.guest.text = "굵게 text".into();
    rig.guest.html = Some(cf_html_from_mac("<html><head><style>b{}</style></head><body><b>굵게</b> text</body></html>"));
    rig.settle();
    let (text, html) = rig.mac.rich_writes.last().cloned().expect("a rich write");
    assert_eq!(text, "굵게 text");
    assert!(html.starts_with("<meta charset=\"utf-8\"><html><head><style>b{}</style>"), "{html}");
    for _ in 0..3 {
        rig.settle();
    }
    assert_eq!(rig.mac.rich_writes.len(), 1, "written once");
    assert!(rig.verbs().is_empty(), "the Mac's own write is not pushed back");
}

#[test]
fn the_same_text_with_new_formatting_is_a_new_copy_both_ways() {
    let mut rig = rig();
    rig.mac.copy_rich("x", "<b>x</b>");
    rig.settle();
    let bold = rig.guest.html.clone();
    rig.mac.copy_rich("x", "<i>x</i>");
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPSETH", "CLIPSETH"]));
    assert_ne!(rig.guest.html, bold, "the italic copy replaced the bold one");
    rig.mac.copy(Some("x"), None);
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPSET"]), "a plain copy of the same text clears the formatting");
    assert_eq!(rig.guest.html, None);

    rig.guest.html = Some(cf_html_from_mac("<u>x</u>"));
    rig.settle();
    assert_eq!(rig.mac.rich_writes.len(), 1, "formatting alone is a guest copy");
    rig.guest.html = None;
    rig.settle();
    assert_eq!(rig.mac.writes.last(), Some(&(Some("x".to_owned()), None)), "and so is losing it");
}

#[test]
fn an_agent_without_cf_html_never_has_the_mac_html_read() {
    let mut rig = Rig::new(true);
    assert!(!rig.h.clip_html);
    rig.mac.copy_rich("plain", "<b>plain</b>");
    rig.settle();
    assert!(rig.wire.iter().any(|line| line == "CLIPX") && !rig.wire.iter().any(|line| line.starts_with("CLIPXH")));
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS", "CLIPSET"]));
    assert_eq!(rig.mac.html_reads, 0);
}

#[test]
fn an_image_copy_carries_no_html_either_way() {
    let mut rig = rig();
    let image = png(4, 4, 100, 5);
    rig.mac.copy(Some("caption"), Some(PasteboardImage::Png(image.clone())));
    rig.mac.html = Some("<img src=x>".into());
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPSET", "PUTBEG", "PUTCHUNK", "PUTEND", "CLIPSETIMG"]));
    assert_eq!(rig.mac.html_reads, 0, "not read beside an image");
    rig.guest.image = Some(png(6, 6, 200, 8));
    rig.guest.html = Some(cf_html_from_mac("<img src=y>"));
    rig.settle();
    rig.settle();
    assert!(rig.mac.rich_writes.is_empty(), "the guest image goes as an image");
    assert!(rig.mac.writes.last().is_some_and(|(_, png)| png.is_some()));
}

#[test]
fn unusable_guest_html_still_syncs_its_text_once() {
    let mut rig = rig();
    let mut names = Vec::new();
    take_lines(&mut names);
    rig.guest.text = "t".into();
    rig.guest.html = Some(b"no CF_HTML header".to_vec());
    rig.settle();
    rig.settle();
    assert_eq!(rig.mac.writes, vec![(Some("t".to_owned()), None)]);
    assert_eq!(
        take_lines(&mut names),
        vec!["BVAGENT CLIPSYNC refused guest-html malformed copy=a", "BVAGENT CLIPSYNC guest->host bytes=1 copy=a"]
    );
    rig.guest.text = "u".into();
    rig.guest.html = Some(vec![b'a'; CLIP_HTML_MAX_BYTES + 1]);
    rig.settle();
    assert_eq!(rig.mac.writes.last(), Some(&(Some("u".to_owned()), None)));
    assert_eq!(
        take_lines(&mut names),
        vec![
            format!("BVAGENT CLIPSYNC refused guest-html too-large bytes={} limit={CLIP_HTML_MAX_BYTES} copy=b", CLIP_HTML_MAX_BYTES + 1),
            "BVAGENT CLIPSYNC guest->host bytes=1 copy=b".to_owned()
        ]
    );
}

#[test]
fn an_agent_that_drops_cf_html_gets_the_queued_copy_as_plain_text() {
    let mut rig = rig();
    rig.h.queue.push_back(ServiceReq::ClipPush("queued".into(), Some(base64_encode(&cf_html_from_mac("<b>q</b>")))));
    rig.h.last_synced.html = Some([1; 32]);
    let caps = format!("CLIPCAPS IMAGE_PNG {}", base64_encode(DIR.as_bytes()));
    rig.h.handle_clip_caps_reply(&caps, rig.now);
    assert!(!rig.h.clip_html && rig.h.last_synced.html.is_none());
    assert!(matches!(rig.h.queue.back(), Some(ServiceReq::ClipPush(text, None)) if text == "queued"));
}
