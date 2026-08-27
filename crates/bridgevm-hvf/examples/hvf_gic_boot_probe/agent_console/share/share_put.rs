//! Host-to-guest shared-file PUT state machine, retry, and reply correlation.

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

pub(in crate::agent_console) fn share_put_reply_matches_phase(
    req: &ServiceReq,
    line: &str,
) -> bool {
    let ServiceReq::SharePut { phase, .. } = req else {
        return false;
    };
    match phase {
        SharePutPhase::Legacy => line == "ERR PUT"
            || line.starts_with("ERR PUT ")
            || line.starts_with("PUTOK "),
        SharePutPhase::Beg => line == "OK PUTBEG"
            || line == "ERR PUTBEG"
            || line.starts_with("ERR PUTBEG "),
        SharePutPhase::Chunk => line.starts_with("OK PUTCHUNK ")
            || line == "ERR PUTCHUNK"
            || line.starts_with("ERR PUTCHUNK "),
        SharePutPhase::End => line.starts_with("PUTOK ")
            || line == "ERR PUTEND"
            || line.starts_with("ERR PUTEND "),
    }
}

#[cfg(test)]
mod tests {
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
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn handle_share_put_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        if !self
            .in_flight
            .as_ref()
            .is_some_and(|(req, _)| share_put_reply_matches_phase(req, line))
        {
            return;
        }
        if line == "OK PUTBEG" {
            self.send_next_share_put_chunk(platform, mem, now);
            return;
        }

        if let Some(seq) = line
            .strip_prefix("OK PUTCHUNK ")
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.advance_share_put_after_chunk(seq, platform, mem, now);
            return;
        }

        if let Some(rest) = line.strip_prefix("PUTOK ") {
            let Some((
                ServiceReq::SharePut {
                    name, bytes, hash, ..
                },
                _,
            )) = self.in_flight.take()
            else {
                return;
            };
            let len = bytes.len();
            let written = rest
                .split_once(' ')
                .and_then(|(_, n)| n.parse::<u64>().ok())
                .unwrap_or(len as u64);
            println!(
                "BVAGENT SHARE host->guest {name} bytes={written} t={}",
                self.t_ms(now)
            );
            if let Some(share) = self.share.as_mut() {
                share.engine.on_put_ok(name, len as u64, hash);
            }
            return;
        }

        if line.starts_with("ERR") {
            let Some((ServiceReq::SharePut { name, .. }, _)) = self.in_flight.take() else {
                return;
            };
            println!("BVAGENT SHARE put-error {name} {line}");
        }
    }
    pub(in crate::agent_console) fn send_next_share_put_chunk(
        &mut self,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        let Some((seq, start, end)) = self.prepare_next_share_put_chunk_payload(now) else {
            return;
        };
        if !self.write_share_put_chunk_line(seq, start, end) {
            return;
        }
        self.last_send = Some(now);
        if let (Some(platform), Some(mem)) = (platform, mem) {
            platform.virtio_console_agent_send(self.service_line_scratch.as_bytes(), mem);
        }
    }
    pub(in crate::agent_console) fn prepare_next_share_put_chunk_payload(
        &mut self,
        now: Instant,
    ) -> Option<(usize, usize, usize)> {
        let Some((
            ServiceReq::SharePut {
                bytes,
                next_chunk,
                phase,
                ..
            },
            sent_at,
        )) = self.in_flight.as_mut()
        else {
            return None;
        };
        if *phase != SharePutPhase::Beg {
            return None;
        }
        let seq = *next_chunk;
        let start = seq.saturating_mul(SHARE_PUT_CHUNK_BYTES);
        let end = (start + SHARE_PUT_CHUNK_BYTES).min(bytes.len());
        if start > end || start >= bytes.len() {
            return None;
        }
        *next_chunk = seq + 1;
        *phase = SharePutPhase::Chunk;
        *sent_at = now;
        Some((seq, start, end))
    }
    pub(in crate::agent_console) fn advance_share_put_after_chunk(
        &mut self,
        seq: usize,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        let Some(line) = self.prepare_share_put_line_after_chunk(seq, now) else {
            return;
        };
        if !self.write_share_put_wire_line(line) {
            return;
        }
        self.last_send = Some(now);
        if let (Some(platform), Some(mem)) = (platform, mem) {
            platform.virtio_console_agent_send(self.service_line_scratch.as_bytes(), mem);
        }
    }
    pub(in crate::agent_console) fn prepare_share_put_line_after_chunk(
        &mut self,
        seq: usize,
        now: Instant,
    ) -> Option<SharePutWireLine> {
        let Some((
            ServiceReq::SharePut {
                bytes,
                next_chunk,
                phase,
                ..
            },
            sent_at,
        )) = self.in_flight.as_mut()
        else {
            return None;
        };
        if *phase != SharePutPhase::Chunk || seq + 1 != *next_chunk {
            return None;
        }
        let nchunks = bytes.len().div_ceil(SHARE_PUT_CHUNK_BYTES);
        let line = if *next_chunk < nchunks {
            let send_seq = *next_chunk;
            let start = send_seq * SHARE_PUT_CHUNK_BYTES;
            let end = (start + SHARE_PUT_CHUNK_BYTES).min(bytes.len());
            *next_chunk = send_seq + 1;
            *phase = SharePutPhase::Chunk;
            SharePutWireLine::Chunk {
                seq: send_seq,
                start,
                end,
            }
        } else {
            *phase = SharePutPhase::End;
            SharePutWireLine::End { nchunks }
        };
        *sent_at = now;
        Some(line)
    }
    pub(in crate::agent_console) fn write_share_put_wire_line(&mut self, line: SharePutWireLine) -> bool {
        match line {
            SharePutWireLine::Chunk { seq, start, end } => {
                self.write_share_put_chunk_line(seq, start, end)
            }
            SharePutWireLine::End { nchunks } => {
                self.service_line_scratch.clear();
                let _ = writeln!(&mut self.service_line_scratch, "PUTEND {nchunks}");
                true
            }
        }
    }
    pub(in crate::agent_console) fn write_share_put_chunk_line(
        &mut self,
        seq: usize,
        start: usize,
        end: usize,
    ) -> bool {
        let Some((ServiceReq::SharePut { bytes, .. }, _)) = self.in_flight.as_ref() else {
            return false;
        };
        if start > end || end > bytes.len() {
            return false;
        }
        self.service_line_scratch.clear();
        let _ = write!(&mut self.service_line_scratch, "PUTCHUNK {seq} ");
        base64_encode_into(&bytes[start..end], &mut self.service_line_scratch);
        self.service_line_scratch.push('\n');
        true
    }
}