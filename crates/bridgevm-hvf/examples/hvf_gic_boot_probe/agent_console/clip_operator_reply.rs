//! The run.log line for an operator CLIPGET (follow-up of 2026-10-23). A CLIPGET
//! from the control file or a scripted command is answered with the guest's
//! clipboard text, which is never printed: the line carries the decoded byte
//! length and the first 16 hex digits of the SHA-256 of those bytes, so a
//! verifier that knows the text it expects can still compare it. The automatic
//! clipboard sync (clip_sync.rs) prints byte counts only, as before.

use super::*;
use sha2::{Digest, Sha256};

/// Hex digits of the SHA-256 that an operator CLIPGET line shows.
const SHA256_PREFIX_HEX: usize = 16;

/// The one bounded line for an operator CLIPGET's `CLIP <b64>` reply.
pub(super) fn operator_clip_line(command: &str, b64: &str) -> String {
    match base64_decode(b64) {
        Ok(bytes) => {
            let mut prefix = String::with_capacity(SHA256_PREFIX_HEX);
            for byte in &Sha256::digest(&bytes)[..SHA256_PREFIX_HEX / 2] {
                let _ = write!(prefix, "{byte:02x}");
            }
            format!(
                "BVAGENT CLIP {command} bytes={} sha256={prefix}",
                bytes.len()
            )
        }
        Err(error) => format!(
            "BVAGENT CLIP {command} base64-error={error:?} payload-bytes={}",
            b64.len()
        ),
    }
}
