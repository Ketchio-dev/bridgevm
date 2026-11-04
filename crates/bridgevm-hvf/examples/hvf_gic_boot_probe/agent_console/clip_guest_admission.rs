//! Admission of a guest CLIP reply before clip_sync.rs may adopt it onto the
//! Mac. Decision of 2026-10-23: a guest text whose decoded size would exceed
//! 8 MiB is refused whole, never truncated. The base64 length is checked before
//! decoding. Malformed base64 and invalid UTF-8 are also refused whole, never
//! converted into an empty or replacement snapshot. Each refused text
//! is counted once and reported in one bounded line that carries no clipboard
//! content. Host-to-guest sync keeps its own `clip_max_bytes` limit.

use super::*;
use std::hash::DefaultHasher;
#[path = "clip_guest_decode.rs"]
mod decode;
#[cfg(test)]
pub(super) use decode::base64_decoded_len;
pub(super) use decode::{decode_guest_clip, GuestClipError};

/// Largest guest clipboard text, in decoded UTF-8 bytes, the Mac may receive.
pub(super) const GUEST_CLIP_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct GuestClipRefusals {
    /// Guest texts refused so far; repeated polls of one text count once.
    pub(super) count: u64,
    /// Fingerprint of the refused reply the unknown baseline stands for.
    last: Option<u64>,
}

/// The bounded refusal record: sizes and counts only, never clipboard text.
/// It names no direction, so the app's run.log parser never reads it as a sync.
pub(super) fn guest_clip_refusal_line(bytes: usize, refusals: u64, t_ms: u128) -> String {
    format!(
        "BVAGENT CLIPSYNC refused guest-text too-large bytes={bytes} \
         limit={GUEST_CLIP_MAX_BYTES} refusals={refusals} t={t_ms}"
    )
}

/// All decode refusals carry fixed reasons and counters, never guest bytes.
pub(super) fn guest_clip_error_line(error: GuestClipError, refusals: u64, t_ms: u128) -> String {
    let reason = match error {
        GuestClipError::TooLarge { bytes } => {
            return guest_clip_refusal_line(bytes, refusals, t_ms)
        }
        GuestClipError::InvalidBase64 => "invalid-base64",
        GuestClipError::InvalidUtf8 => "invalid-utf8",
    };
    format!("BVAGENT CLIPSYNC refused guest-text {reason} refusals={refusals} t={t_ms}")
}

impl AgentConsoleHarness {
    /// The decoded guest text, or None when decoding was refused (logged once).
    pub(super) fn admit_guest_text(&mut self, b64: &str, now: Instant) -> Option<String> {
        let text = match decode_guest_clip(b64) {
            Ok(text) => text,
            Err(error) => {
                self.refuse_guest_clip(b64, error, now);
                return None;
            }
        };
        self.last_synced.refusals.last = None;
        Some(text)
    }

    /// The guest text of a CLIP reply that clip_sync.rs may adopt, or None when
    /// decoding was refused or it became the baseline of a pending rebaseline.
    pub(super) fn admit_guest_clip(&mut self, b64: &str, now: Instant) -> Option<String> {
        let text = self.admit_guest_text(b64, now)?;
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
    fn refuse_guest_clip(&mut self, b64: &str, error: GuestClipError, now: Instant) {
        let fingerprint = Some(guest_clip_fingerprint(b64, &mut DefaultHasher::new()));
        let t_ms = self.t_ms(now);
        let baseline = &mut self.last_synced;
        // Each interval re-polls an unchanged guest clipboard: one refusal.
        if baseline.text.is_some() || baseline.refusals.last != fingerprint {
            baseline.refusals.count = baseline.refusals.count.saturating_add(1);
            let refusals = baseline.refusals.count;
            println!("{}", guest_clip_error_line(error, refusals, t_ms));
        }
        baseline.text = None;
        baseline.refusals.last = fingerprint;
    }
}
