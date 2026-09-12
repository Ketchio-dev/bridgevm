use super::*;

// Framing and byte validation are unchanged; only input command labels redact payloads.
impl AgentConsoleHarness {
    pub(super) fn finish_out(&mut self, command: &str, rest: &str) {
        let command = input_receipt_label::label(command);
        let Some(accum) = self.out_accum.take() else {
            println!(
                "BVAGENT CMD {command} exit=-1\n<chunked output protocol error: OUTEND without OUTBEG>\nBVAGENT END {command}"
            );
            return;
        };
        let end_count = rest.trim().parse::<usize>().ok();
        let valid = accum.valid
            && end_count == Some(accum.nchunks)
            && accum.chunks_seen == accum.nchunks
            && accum.bytes.len() == accum.total;
        if valid {
            let text = String::from_utf8_lossy(&accum.bytes);
            println!(
                "BVAGENT CMD {command} exit={}\n{text}\nBVAGENT END {command}",
                accum.exit_code
            );
        } else {
            println!(
                "BVAGENT CMD {command} exit=-1\n<chunked output protocol error: declared-exit={} bytes={}/{} chunks={}/{} end={}>\nBVAGENT END {command}",
                accum.exit_code,
                accum.bytes.len(),
                accum.total,
                accum.chunks_seen,
                accum.nchunks,
                end_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "invalid".to_string())
            );
        }
    }
}
