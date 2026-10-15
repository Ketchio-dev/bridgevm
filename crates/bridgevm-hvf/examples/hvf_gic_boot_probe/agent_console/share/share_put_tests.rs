//! Chunked PUT retransmit and phase-specific reply correlation.

use super::*;

fn chunked_req() -> ServiceReq {
    share_put_req(
        "big.bin".into(),
        vec![7u8; SHARE_PUT_CHUNK_BYTES + 1],
        1,
    )
}

#[test]
fn chunked_retransmit_restarts_from_putbeg() {
    let mut req = chunked_req();
    if let ServiceReq::SharePut {
        next_chunk, phase, ..
    } = &mut req
    {
        *next_chunk = 2;
        *phase = SharePutPhase::End;
    }
    let ServiceReq::SharePut {
        next_chunk, phase, ..
    } = rewind_share_put_for_retransmit(req)
    else {
        panic!("expected SharePut");
    };
    assert_eq!(next_chunk, 0);
    assert_eq!(phase, SharePutPhase::Beg);
}

#[test]
fn delayed_completion_cannot_finish_a_restarted_transfer() {
    let mut req = chunked_req();
    if let ServiceReq::SharePut { phase, .. } = &mut req {
        *phase = SharePutPhase::End;
    }
    assert!(share_put_reply_matches_phase(&req, "PUTOK Zm9v 24577"));
    let req = rewind_share_put_for_retransmit(req);
    assert!(!share_put_reply_matches_phase(&req, "PUTOK Zm9v 24577"));
    assert!(share_put_reply_matches_phase(&req, "OK PUTBEG"));
}

#[test]
fn acknowledgements_are_phase_specific() {
    let mut req = chunked_req();
    assert!(share_put_reply_matches_phase(&req, "OK PUTBEG"));
    assert!(!share_put_reply_matches_phase(&req, "OK PUTCHUNK 0"));
    assert!(!share_put_reply_matches_phase(&req, "ERR PUTEND stale"));
    if let ServiceReq::SharePut { phase, .. } = &mut req {
        *phase = SharePutPhase::Chunk;
    }
    assert!(share_put_reply_matches_phase(&req, "OK PUTCHUNK 0"));
    assert!(!share_put_reply_matches_phase(&req, "OK PUTBEG"));
    if let ServiceReq::SharePut { phase, .. } = &mut req {
        *phase = SharePutPhase::End;
    }
    assert!(share_put_reply_matches_phase(&req, "ERR PUTEND disk-full"));
    assert!(!share_put_reply_matches_phase(&req, "ERR PUTCHUNK stale"));
}

#[test]
fn legacy_errors_do_not_match_chunked_phases() {
    let legacy = share_put_req("small.bin".into(), vec![1, 2, 3], 2);
    assert!(share_put_reply_matches_phase(&legacy, "ERR PUT access-denied"));
    assert!(!share_put_reply_matches_phase(&legacy, "ERR PUTBEG stale"));
    assert!(!share_put_reply_matches_phase(&legacy, "ERR PUTEND stale"));
}
