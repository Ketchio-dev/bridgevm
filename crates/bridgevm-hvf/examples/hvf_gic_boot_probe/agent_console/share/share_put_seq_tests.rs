//! The sequence number in a guest `OK PUTCHUNK <seq>` reply is untrusted. One
//! that does not acknowledge the chunk on the wire is ignored, never computed
//! with: release builds keep overflow checks, so `seq + 1` on `usize::MAX`
//! panicked on the probe's main (CPU0) thread while it held the platform lock.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

/// A chunked PUT whose `PUTCHUNK 0` is on the wire, awaiting its acknowledgement.
fn chunk_zero_in_flight(t0: Instant) -> AgentConsoleHarness {
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    let bytes = vec![7u8; SHARE_PUT_CHUNK_BYTES * 2 + 1];
    h.in_flight = Some((share_put_req("big.bin".into(), bytes, 1), t0));
    h.handle_service_reply("OK PUTBEG", None, None, t0);
    assert!(h.service_line_scratch.starts_with("PUTCHUNK 0 "));
    h
}

fn chunk_progress(h: &AgentConsoleHarness) -> (usize, SharePutPhase, Instant) {
    match h.in_flight.as_ref() {
        Some((
            ServiceReq::SharePut {
                next_chunk, phase, ..
            },
            sent_at,
        )) => (*next_chunk, *phase, *sent_at),
        _ => panic!("the chunked PUT is no longer in flight"),
    }
}

#[test]
fn putchunk_ack_for_a_chunk_not_on_the_wire_is_ignored_even_at_usize_max() {
    let t0 = Instant::now();
    for seq in [usize::MAX.to_string(), "1".into(), "7".into()] {
        let mut h = chunk_zero_in_flight(t0);
        let sent = h.service_line_scratch.clone();
        let reply = format!("OK PUTCHUNK {seq}");
        h.handle_service_reply(&reply, None, None, t0 + Duration::from_millis(1));
        assert_eq!(chunk_progress(&h), (1, SharePutPhase::Chunk, t0), "{reply}");
        assert_eq!(
            h.service_line_scratch, sent,
            "{reply} put another line on the wire"
        );
        // The genuine acknowledgement still advances the same transfer.
        h.handle_service_reply("OK PUTCHUNK 0", None, None, t0 + Duration::from_millis(2));
        assert!(
            h.service_line_scratch.starts_with("PUTCHUNK 1 "),
            "after {reply}"
        );
        assert_eq!(chunk_progress(&h).0, 2, "after {reply}");
    }
}
