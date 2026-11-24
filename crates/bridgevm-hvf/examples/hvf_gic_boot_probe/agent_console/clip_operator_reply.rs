//! Operator CLIPGET and CLIPSET log fingerprints so a verifier can match
//! the expected content without printing it. Automatic clipboard sync
//! (clip_sync.rs) prints byte counts.

use super::*;
use sha2::{Digest, Sha256};

/// Hex digits of the SHA-256 that an operator CLIPGET line shows.
const SHA256_PREFIX_HEX: usize = 16;

/// The one bounded line for an operator CLIPGET's `CLIP <b64>` reply.
pub(super) fn operator_clip_line(command: &str, b64: &str) -> String {
    format!("BVAGENT CLIP {command} {}", clipboard_fields(b64))
}

/// CLIPSET replies can carry an exception message. Log only the outcome and
/// host-calculated fingerprint, even if the guest echoes the input on failure.
pub(super) fn operator_clipset_reply(command: &str, line: &str) -> Option<ReplyProgress> {
    let mut fields = command.trim().splitn(2, char::is_whitespace);
    if fields.next() != Some("CLIPSET") {
        return None;
    }
    let status = if line == "OK CLIPSET" {
        "OK CLIPSET"
    } else if line == "ERR" || line.starts_with("ERR ") {
        "ERR CLIPSET"
    } else {
        return Some(ReplyProgress::Ignored);
    };
    let payload = fields.next().unwrap_or_default().trim();
    println!("BVAGENT CLIPSET {} -> {status}", clipboard_fields(payload));
    Some(ReplyProgress::Complete)
}

fn clipboard_fields(b64: &str) -> String {
    match base64_decode(b64) {
        Ok(bytes) => {
            let mut prefix = String::with_capacity(SHA256_PREFIX_HEX);
            for byte in &Sha256::digest(&bytes)[..SHA256_PREFIX_HEX / 2] {
                let _ = write!(prefix, "{byte:02x}");
            }
            format!("bytes={} sha256={prefix}", bytes.len())
        }
        Err(error) => format!("base64-error={error:?} payload-bytes={}", b64.len()),
    }
}
