//! Fingerprint of a refused guest CLIP reply (clip_guest_admission.rs), so an
//! oversized text that stays on the guest clipboard is counted only once. It
//! reads the payload's length and its first and last 4 KiB, not all of it: the
//! guest is re-polled every interval, and a payload can reach the 32 MiB line
//! limit. Two payloads that agree on all three are taken for one text; that
//! collision costs one uncounted refusal, never an admitted text.

use std::hash::{Hash, Hasher};

/// Bytes read from each end of a refused payload.
const FINGERPRINT_EDGE_BYTES: usize = 4096;

/// The fingerprint of a refused CLIP payload, reading at most 8 KiB of it.
pub(super) fn guest_clip_fingerprint(b64: &str, hasher: &mut impl Hasher) -> u64 {
    let bytes = b64.as_bytes();
    let edge = bytes.len().min(FINGERPRINT_EDGE_BYTES);
    let (head, tail) = (&bytes[..edge], &bytes[bytes.len() - edge..]);
    (bytes.len(), head, tail).hash(hasher);
    hasher.finish()
}
