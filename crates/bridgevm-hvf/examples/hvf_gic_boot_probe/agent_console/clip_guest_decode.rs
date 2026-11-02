//! Strict guest clipboard decoding; errors contain reasons and counts only.

use super::{base64_decode, GUEST_CLIP_MAX_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agent_console) enum GuestClipError {
    TooLarge { bytes: usize },
    InvalidBase64,
    InvalidUtf8,
}

/// Exact decoded length for valid padded base64; an upper bound otherwise.
pub(in crate::agent_console) fn base64_decoded_len(b64: &str) -> usize {
    let padding = b64.bytes().rev().take(2).take_while(|b| *b == b'=').count();
    (b64.len() / 4 * 3).saturating_sub(padding)
}

/// Keep malformed transport and non-UTF8 bytes out of clipboard state changes.
pub(in crate::agent_console) fn decode_guest_clip(b64: &str) -> Result<String, GuestClipError> {
    let bound = base64_decoded_len(b64);
    if bound > GUEST_CLIP_MAX_BYTES {
        return Err(GuestClipError::TooLarge { bytes: bound });
    }
    let bytes = base64_decode(b64).map_err(|_| GuestClipError::InvalidBase64)?;
    if bytes.len() > GUEST_CLIP_MAX_BYTES {
        return Err(GuestClipError::TooLarge { bytes: bytes.len() });
    }
    String::from_utf8(bytes).map_err(|_| GuestClipError::InvalidUtf8)
}
