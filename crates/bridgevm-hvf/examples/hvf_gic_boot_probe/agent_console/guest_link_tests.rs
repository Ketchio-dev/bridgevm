//! OPENURL lines from the agent's link watch, and LINKWATCH itself.

use super::super::desktop_switch::tests::service_harness;
use super::super::*;
use super::*;

const AT: u128 = 1_789_000_000_123;

fn line_for(url: &[u8]) -> String {
    open_url_line(&base64_encode(url), AT)
}

#[test]
fn a_link_is_logged_with_its_time_and_base64() {
    let long = format!("https://example.com/{}", "a".repeat(2048 - 20));
    for url in [
        "https://example.com/?a=1&b=%22x%22#top",
        "http://intranet:8080/",
        "HTTPS://EXAMPLE.COM/",
        "https://\u{c608}\u{c2dc}.kr/\u{acbd}\u{b85c}",
        long.as_str(),
    ] {
        let b64 = base64_encode(url.as_bytes());
        assert_eq!(line_for(url.as_bytes()), format!("BVAGENT OPENURL at={AT} {b64}"), "{url}");
    }
}

#[test]
fn anything_else_is_refused_without_the_guests_text() {
    let too_long = format!("https://example.com/{}", "a".repeat(2049 - 20));
    for (url, reason) in [
        (too_long.as_bytes(), "too-long"),
        (b"https://x/\xff".as_slice(), "utf8"),
        (b"https://x/a b", "character"),
        (b"https://x/a\tb", "character"),
        (b"https://x/\n", "character"),
        ("https://x/\u{a0}".as_bytes(), "character"),
        (b"https://x/\x01", "character"),
        (b"https://x/\x7f", "character"),
        (b"javascript:alert('http://x')", "scheme"),
        (b"file:///etc/passwd", "scheme"),
        (b"smb://server/share", "scheme"),
        (b"ftp://example.com/", "scheme"),
        (b"xhttp://example.com/", "scheme"),
        (b"https://", "scheme"),
        (b"example.com", "scheme"),
        (b"", "scheme"),
    ] {
        assert_eq!(
            line_for(url),
            format!("BVAGENT OPENURL refused reason={reason}"),
            "{}",
            String::from_utf8_lossy(url)
        );
    }
    for b64 in ["not base64!", "aHR0cHM6Ly94Lw"] {
        assert_eq!(open_url_line(b64, AT), "BVAGENT OPENURL refused reason=base64", "{b64}");
    }
    // A guest line up to 32 MiB is never decoded past the longest URL.
    let huge = "A".repeat(64 * 1024);
    assert_eq!(open_url_line(&huge, AT), "BVAGENT OPENURL refused reason=too-long");
}

fn reply(h: &mut AgentConsoleHarness, line: &str, now: Instant) {
    h.handle_service_reply_with_sender(line, now, &mut |_| panic!("a reply sent a line"));
}

fn next_wire(h: &mut AgentConsoleHarness, now: Instant) -> Option<String> {
    h.service_enqueue(now);
    let mut sent = None;
    h.service_pump_with_sender(now, &mut |bytes| {
        sent = Some(String::from_utf8_lossy(bytes).into_owned());
    });
    sent
}

fn running() -> (AgentConsoleHarness, Instant) {
    let mut h = service_harness();
    h.state = AgentConsoleState::Service;
    let now = Instant::now();
    h.last_send = Some(now);
    (h, now)
}

fn out(text: &str) -> String {
    format!("OUT 0 {}", base64_encode(text.as_bytes()))
}

/// The agent writes OPENURL between replies, like CLIPCHANGED: the request on the wire keeps
/// waiting for its own reply, and nothing is queued or sent for the link.
#[test]
fn a_link_between_replies_leaves_the_request_in_flight() {
    let (mut h, now) = running();
    h.queue.push_back(ServiceReq::Ctl("whoami".into()));
    assert!(next_wire(&mut h, now).is_some_and(|line| line.starts_with("RUN ")));
    let link = format!("OPENURL {}", base64_encode(b"https://example.com/"));
    for line in [link.as_str(), "OPENURL bad", "OPENURL "] {
        reply(&mut h, line, now);
        assert!(matches!(h.in_flight, Some((ServiceReq::Ctl(ref c), _)) if c == "whoami"), "{line}");
        assert!(h.queue.is_empty(), "{line}");
    }
    reply(&mut h, &out("bv\\user\r\n"), now);
    assert!(h.in_flight.is_none());
    assert_eq!(next_wire(&mut h, now), None);
}

/// LINKWATCH goes as itself, and an agent restart (a guest tools update) gets it again.
#[test]
fn the_link_watch_is_asked_again_after_an_agent_restart() {
    let (mut h, now) = running();
    reply(&mut h, "READY BRIDGEVM v3-share2", now);
    let watches = |h: &AgentConsoleHarness| {
        h.queue
            .iter()
            .filter(|req| matches!(req, ServiceReq::Ctl(c) if c == WATCH_VERB))
            .count()
    };
    assert_eq!(watches(&h), 0, "never asked: not asked after a restart");
    h.queue.clear();

    h.queue.push_back(ServiceReq::Ctl(WATCH_VERB.into()));
    assert_eq!(next_wire(&mut h, now).as_deref(), Some("LINKWATCH\n"));
    reply(&mut h, &out("BVLINKS watch=on"), now);
    assert!(h.in_flight.is_none());
    for _ in 0..2 {
        reply(&mut h, "READY BRIDGEVM v3-share2", now);
        assert_eq!(watches(&h), 1);
    }
    h.queue.retain(|req| matches!(req, ServiceReq::Ctl(c) if c == WATCH_VERB));
    assert_eq!(next_wire(&mut h, now).as_deref(), Some("LINKWATCH\n"));
    // Cut by another restart before its reply: asked once more.
    reply(&mut h, "READY BRIDGEVM v3-share2", now);
    assert_eq!(watches(&h), 1);
}
