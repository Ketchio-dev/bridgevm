//! Host-to-guest shared-file PUT construction and retry state.

use super::*;

pub(in crate::agent_console) fn share_put_req(
    name: String,
    bytes: Vec<u8>,
    hash: u64,
) -> ServiceReq {
    let phase = if bytes.len() <= SHARE_PUT_CHUNK_BYTES {
        SharePutPhase::Legacy
    } else {
        SharePutPhase::Beg
    };
    ServiceReq::SharePut {
        name,
        bytes,
        hash,
        next_chunk: 0,
        phase,
    }
}

pub(in crate::agent_console) fn rewind_share_put_for_retransmit(
    mut req: ServiceReq,
) -> ServiceReq {
    if let ServiceReq::SharePut {
        next_chunk, phase, ..
    } = &mut req
    {
        if *phase != SharePutPhase::Legacy {
            *next_chunk = 0;
            *phase = SharePutPhase::Beg;
        }
    }
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_retransmit_restarts_from_putbeg() {
        let bytes = vec![7u8; SHARE_PUT_CHUNK_BYTES + 1];
        let mut req = share_put_req("big.bin".into(), bytes, 1);
        if let ServiceReq::SharePut {
            next_chunk, phase, ..
        } = &mut req
        {
            *next_chunk = 1;
            *phase = SharePutPhase::Chunk;
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
}
