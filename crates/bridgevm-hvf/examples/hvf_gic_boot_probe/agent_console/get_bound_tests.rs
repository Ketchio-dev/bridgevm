use super::super::agent_console_tests::tests::harness;
use super::*;

#[test]
fn oversized_get_declaration_reserves_nothing_and_never_completes() {
    let mut h = harness();
    h.begin_get(&format!("{} {} 1", base64_encode(b"C:\\f"), usize::MAX));
    h.accum_get_chunk(&format!("0 {}", base64_encode(b"foo")));
    let accum = h.get_accum.as_ref().unwrap();
    assert_eq!(accum.bytes.capacity(), 0);
    assert!(accum.bytes.is_empty());
}

#[test]
fn get_chunks_past_the_declared_total_are_not_accumulated() {
    let mut h = harness();
    h.begin_get(&format!("{} 3 1", base64_encode(b"C:\\f")));
    for seq in 0..1000 {
        h.accum_get_chunk(&format!("{seq} {}", base64_encode(b"foo")));
    }
    let accum = h.get_accum.as_ref().unwrap();
    assert_eq!(accum.bytes, b"foo");
    assert_eq!(accum.chunks_seen, 1);
}

#[test]
fn unterminated_agent_reply_stops_buffering_at_the_line_bound() {
    let mut h = harness();
    let flood = vec![b'A'; 1024 * 1024];
    for _ in 0..(MAX_AGENT_LINE_BYTES / flood.len()) + 2 {
        h.inbound_scratch.extend_from_slice(&flood);
        h.frame_inbound_lines();
        assert!(h.line_scratch.is_empty());
    }
    assert!(h.framer.pending.len() <= MAX_AGENT_LINE_BYTES + 1);

    // The framer resynchronises at the next newline.
    h.inbound_scratch.extend_from_slice(b"\nPONG\n");
    h.frame_inbound_lines();
    assert_eq!(h.line_scratch.as_slice(), ["PONG"]);
}
