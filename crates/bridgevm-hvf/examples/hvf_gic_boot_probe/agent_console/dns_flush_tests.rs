//! The guest DNS cache flush that follows a NAT resolver switch.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

fn resident() -> AgentConsoleHarness {
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    h
}

fn flushes(h: &AgentConsoleHarness) -> usize {
    let in_flight = h.in_flight.iter().map(|(req, _)| req);
    h.queue
        .iter()
        .chain(in_flight)
        .filter(|req| is_dns_flush(req))
        .count()
}

#[test]
fn each_resolver_switch_flushes_the_guest_cache_once() {
    let mut h = resident();
    let now = Instant::now();
    h.enqueue_dns_flush();
    assert_eq!(flushes(&h), 0, "no switch, no flush");

    // Two switches before the service ran: one flush.
    h.dns_flush.seen = 2;
    h.enqueue_dns_flush();
    h.enqueue_dns_flush();
    assert_eq!(flushes(&h), 1);
    let mut sent = Vec::new();
    h.service_pump_with_sender(now, &mut |bytes| {
        sent.push(String::from_utf8(bytes.to_vec()).unwrap())
    });
    assert_eq!(sent.len(), 1);
    let b64 = sent[0].strip_prefix("RUN ").unwrap().trim_end();
    assert_eq!(base64_decode(b64).unwrap(), DNS_FLUSH_COMMAND.as_bytes());

    // A switch while it runs gets exactly one follow-up.
    h.dns_flush.seen = 3;
    h.enqueue_dns_flush();
    h.enqueue_dns_flush();
    assert_eq!(flushes(&h), 2);
    h.handle_service_reply_with_sender("OUT 0 ", now, &mut |_| {});
    assert!(h.in_flight.is_none());
    assert_eq!(flushes(&h), 1);
    h.enqueue_dns_flush();
    assert_eq!(flushes(&h), 1);

    // cmd.exe's 8,191 characters less bvagent's `"C:\Windows\system32\cmd.exe" /c ""`.
    assert!(DNS_FLUSH_COMMAND.len() < 8_156);
}
