//! Admission of a guest CLIP reply before clip_sync.rs may adopt it onto the
//! Mac. Decision of 2026-10-23: a guest text whose decoded size would exceed
//! 8 MiB is refused whole, never truncated. The base64 length is checked before
//! decoding and the lossy UTF-8 size before the text is built. Each refused text
//! is counted once and reported in one bounded line that carries no clipboard
//! content. Host-to-guest sync keeps its own `clip_max_bytes` limit.

use super::*;
use std::hash::DefaultHasher;

/// Largest guest clipboard text, in decoded UTF-8 bytes, the Mac may receive.
pub(super) const GUEST_CLIP_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct GuestClipRefusals {
    /// Guest texts refused so far; repeated polls of one text count once.
    pub(super) count: u64,
    /// Fingerprint of the refused reply the unknown baseline stands for.
    last: Option<u64>,
}

/// Decoded size of a padded base64 payload: exact when it is valid, an upper
/// bound otherwise (base64_decode then rejects it).
pub(super) fn base64_decoded_len(b64: &str) -> usize {
    let padding = b64.bytes().rev().take(2).take_while(|b| *b == b'=').count();
    (b64.len() / 4 * 3).saturating_sub(padding)
}

/// The guest text of a CLIP payload, or the decoded size it is refused at.
/// A malformed payload within the limit decodes to empty text, as before.
pub(super) fn decode_guest_clip(b64: &str) -> Result<String, usize> {
    let bound = base64_decoded_len(b64);
    if bound > GUEST_CLIP_MAX_BYTES {
        return Err(bound);
    }
    let bytes = match String::from_utf8(base64_decode(b64).unwrap_or_default()) {
        Ok(text) => return Ok(text),
        Err(error) => error.into_bytes(),
    };
    // Lossy decoding puts one U+FFFD (3 bytes) per invalid sequence.
    let lossy: usize = bytes
        .utf8_chunks()
        .map(|chunk| chunk.valid().len() + if chunk.invalid().is_empty() { 0 } else { 3 })
        .sum();
    if lossy > GUEST_CLIP_MAX_BYTES {
        return Err(lossy);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The bounded refusal record: sizes and counts only, never clipboard text.
/// It names no direction, so the app's run.log parser never reads it as a sync.
pub(super) fn guest_clip_refusal_line(bytes: usize, refusals: u64, t_ms: u128) -> String {
    format!(
        "BVAGENT CLIPSYNC refused guest-text too-large bytes={bytes} \
         limit={GUEST_CLIP_MAX_BYTES} refusals={refusals} t={t_ms}"
    )
}

impl AgentConsoleHarness {
    /// The guest text of a CLIP reply that clip_sync.rs may adopt, or None when
    /// it was refused for size or became the baseline of a pending rebaseline.
    pub(super) fn admit_guest_clip(&mut self, b64: &str, now: Instant) -> Option<String> {
        let text = match decode_guest_clip(b64) {
            Ok(text) => text,
            Err(bytes) => {
                self.refuse_guest_clip(b64, bytes, now);
                return None;
            }
        };
        self.last_synced.refusals.last = None;
        if !std::mem::take(&mut self.last_synced.rebaseline) {
            return Some(text);
        }
        if !text.is_empty() {
            self.last_synced.text = Some(normalize_clip(&text));
        }
        None
    }

    /// The Mac keeps its copy and the guest's text becomes unknown, so the next
    /// Mac copy is pushed and the next guest copy is adopted. A pending
    /// rebaseline stays pending: a paste landing after this poll is learned,
    /// not published.
    fn refuse_guest_clip(&mut self, b64: &str, bytes: usize, now: Instant) {
        let fingerprint = Some(guest_clip_fingerprint(b64, &mut DefaultHasher::new()));
        let t_ms = self.t_ms(now);
        let baseline = &mut self.last_synced;
        // Each interval re-polls an unchanged guest clipboard: one refusal.
        if baseline.text.is_some() || baseline.refusals.last != fingerprint {
            baseline.refusals.count = baseline.refusals.count.saturating_add(1);
            let refusals = baseline.refusals.count;
            println!("{}", guest_clip_refusal_line(bytes, refusals, t_ms));
        }
        baseline.text = None;
        baseline.refusals.last = fingerprint;
    }
}
