//! A control-file request that starts with a raw protocol verb must leave the
//! lockstep wire slot on that verb's terminal reply. Control-file requests are
//! never retransmitted, so an unrecognised terminal reply wedged the relay.
#![cfg(test)]

use super::agent_console_tests::tests::harness;
use super::*;

fn service_harness_with_ctl(label: &str, ctl: &[u8]) -> AgentConsoleHarness {
    let path = std::env::temp_dir().join(format!("bvagent-ctl-{label}-{}.txt", std::process::id()));
    std::fs::write(&path, ctl).unwrap();
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    h.ctl_path = Some(path.to_string_lossy().into_owned());
    h.ingest_ctl();
    let _ = std::fs::remove_file(&path);
    h
}

/// Send the next queued request exactly as `service_pump` does, without a
/// platform handle, and return its wire line.
fn send_next(h: &mut AgentConsoleHarness, now: Instant) -> String {
    assert!(h.in_flight.is_none(), "wire slot still occupied");
    let req = h.queue.pop_front().expect("queued request");
    assert!(h.write_service_req_line(&req, now));
    h.in_flight = Some((req, now));
    h.service_line_scratch.clone()
}

#[test]
fn ctl_ping_is_completed_by_pong_and_does_not_wedge_later_commands() {
    let mut h = service_harness_with_ctl("ping", b"PING 127.0.0.1\nwhoami\n");
    let now = Instant::now();
    assert_eq!(send_next(&mut h, now), "PING 127.0.0.1\n");
    h.handle_service_reply("PONG", None, None, now);
    assert!(
        h.in_flight.is_none(),
        "PONG left the control-file PING in flight"
    );
    assert!(send_next(&mut h, now).starts_with("RUN "));
}

#[test]
fn ctl_del_error_reply_releases_the_wire_slot() {
    let mut h = service_harness_with_ctl("del", b"DEL C:\\x\n");
    let now = Instant::now();
    assert_eq!(send_next(&mut h, now), "DEL C:\\x\n");
    h.handle_service_reply(
        "ERR DEL Invalid length for a Base-64 char array",
        None,
        None,
        now,
    );
    assert!(h.in_flight.is_none());
}

#[test]
fn ctl_window_verb_unknown_to_an_older_agent_releases_the_wire_slot() {
    let mut h = service_harness_with_ctl("winfocus", b"WINFOCUS 42\n");
    let now = Instant::now();
    assert_eq!(send_next(&mut h, now), "WINFOCUS 42\n");
    let unknown = format!("OUT 255 {}", base64_encode(b"unknown token: WINFOCUS"));
    h.handle_service_reply(&unknown, None, None, now);
    assert!(
        h.in_flight.is_none(),
        "an older agent's unknown-token reply wedged WINFOCUS"
    );
}

#[test]
fn stray_pong_does_not_complete_a_shell_command() {
    let mut h = service_harness_with_ctl("stray", b"whoami\n");
    let now = Instant::now();
    assert!(send_next(&mut h, now).starts_with("RUN "));
    h.handle_service_reply("PONG", None, None, now);
    assert!(
        h.in_flight.is_some(),
        "a stray PONG must not complete a RUN request"
    );
}

#[test]
fn scripted_ping_is_completed_by_pong() {
    let mut h = harness();
    assert!(matches!(
        h.handle_reply_line("PONG", "PING 127.0.0.1"),
        ReplyProgress::Complete
    ));
}
