//! The Mac app appends one ordered-input event per control-file line, up to a
//! 65,536-byte UTF-8 text event: the per-event size the guest advertises in
//! BVINPUT_CAPS and accepts per TEXTINPUT request. The relay must forward that
//! whole line; dropping it at framing loses the text with no guest receipt.
#![cfg(test)]

use super::agent_console_tests::tests::harness;
use super::*;

const ID: &str = "12345678-1234-1234-1234-123456789abc";

fn ingest_service_ctl(label: &str, chunks: &[&[u8]]) -> AgentConsoleHarness {
    let name = format!("bvagent-ctl-input-{label}-{}.txt", std::process::id());
    let path = std::env::temp_dir().join(name);
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    h.ctl_path = Some(path.to_string_lossy().into_owned());
    let mut written = Vec::new();
    for chunk in chunks {
        written.extend_from_slice(chunk);
        std::fs::write(&path, &written).unwrap();
        while h.ctl_offset < written.len() as u64 {
            h.ingest_ctl();
        }
    }
    let _ = std::fs::remove_file(&path);
    h
}

/// Queued control commands, long ones shortened so failures stay readable.
fn queued_ctl(h: &AgentConsoleHarness) -> Vec<String> {
    let summary = |c: &str| match c.get(..47) {
        Some(head) if c.len() > 64 => format!("{head}... len={}", c.len()),
        _ => c.to_string(),
    };
    h.queue
        .iter()
        .filter_map(|req| match req {
            ServiceReq::Ctl(command) => Some(summary(command)),
            _ => None,
        })
        .collect()
}

/// The longest line HvfGuestInputEncoding produces: 65,536 UTF-8 bytes of text.
fn maximum_text_input_line() -> String {
    let text = "\u{1F642}".repeat(16_384);
    assert_eq!(text.len(), 65_536);
    format!("TEXTINPUT {ID} {}", base64_encode(text.as_bytes()))
}

#[test]
fn maximum_text_input_line_is_queued_and_sent_verbatim() {
    let line = maximum_text_input_line();
    assert_eq!(line.len(), 87_431);
    let ctl = format!("{line}\nPING\n");
    let mut h = ingest_service_ctl("max-text", &[ctl.as_bytes()]);
    let expected = format!("TEXTINPUT {ID} ... len=87431");
    assert_eq!(queued_ctl(&h), [expected.as_str(), "PING"]);

    // Sent raw, not RUN-wrapped: the relay's input classifier accepted it.
    let req = h.queue.pop_front().unwrap();
    assert!(h.write_service_req_line(&req, Instant::now()));
    assert!(h.service_line_scratch == format!("{line}\n"));
}

#[test]
fn maximum_text_input_line_split_across_polls_is_queued() {
    let line = maximum_text_input_line();
    let ctl = format!("{line}\r\n");
    let (head, tail) = ctl.as_bytes().split_at(40_000);
    let h = ingest_service_ctl("max-text-split", &[head, tail]);
    assert_eq!(queued_ctl(&h).len(), 1);
    assert!(matches!(h.queue.front(), Some(ServiceReq::Ctl(c)) if *c == line));
}

#[test]
fn lines_past_the_app_input_guard_are_still_rejected_and_resynchronized() {
    // HvfEngineSession refuses ordered-input lines over 87,434 bytes.
    let too_long = "x".repeat(87_435);
    let huge = "x".repeat(4 * 1024 * 1024);
    let ctl = format!("{too_long}\n{huge}\nPING\n");
    let h = ingest_service_ctl("over-bound", &[ctl.as_bytes()]);
    assert_eq!(queued_ctl(&h), ["PING"]);
    let buffered = h.ctl_framer.pending.capacity();
    assert!(buffered < 256 * 1024, "4 MiB line buffered: {buffered}");
}
