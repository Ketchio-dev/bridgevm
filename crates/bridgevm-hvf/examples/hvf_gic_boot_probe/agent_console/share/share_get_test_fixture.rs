//! Feed fetched bytes through the same full guest-path replies as the resident wire.

use super::*;

pub(in crate::agent_console) fn fetched(h: &mut AgentConsoleHarness, name: &str, bytes: &[u8]) {
    let now = Instant::now();
    let request = ServiceReq::ShareGet {
        name: name.to_owned(),
    };
    let wire = h
        .service_req_line(&request, now)
        .expect("share GET wire")
        .to_owned();
    h.in_flight = Some((request, now));
    let path = wire
        .strip_prefix("GET ")
        .and_then(|line| line.strip_suffix('\n'))
        .expect("GET path argument");
    // bvagent.ps1 GET replies use 24 KiB raw chunks.
    let chunk_bytes = 24 * 1024;
    let chunks = bytes.len().div_ceil(chunk_bytes);
    h.handle_service_reply(
        &format!("GETBEG {path} {} {chunks}", bytes.len()),
        None,
        None,
        now,
    );
    for (sequence, chunk) in bytes.chunks(chunk_bytes).enumerate() {
        h.handle_service_reply(
            &format!("GETCHUNK {sequence} {}", base64_encode(chunk)),
            None,
            None,
            now,
        );
    }
    h.handle_service_reply(&format!("GETEND {chunks}"), None, None, now);
}
