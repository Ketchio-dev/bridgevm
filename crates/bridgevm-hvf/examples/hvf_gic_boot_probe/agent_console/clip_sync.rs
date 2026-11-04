//! Clipboard auto-sync between the Mac pasteboard and the guest agent.
//!
//! The baseline is the text the guest is known to hold. It advances to a host
//! copy only on CLIPSET OK, so a copy the guest never received (oversized, ERR,
//! dropped by an agent restart, or overtaken by an earlier CLIPGET) cannot make
//! the guest's older text look like a new guest copy and revert the Mac. An
//! app paste through the guest clipboard re-learns it too (clip_paste.rs).

use super::*;

#[cfg(test)]
#[path = "clip_sync_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct ClipBaseline {
    /// Normalized (LF) guest text last observed, adopted or confirmed.
    pub(super) text: Option<String>,
    /// A host copy failed to reach the guest, or the app pasted through it: the
    /// next CLIPGET becomes the baseline instead of being adopted onto the Mac.
    pub(super) rebaseline: bool,
    /// Guest texts refused for size (clip_guest_admission.rs).
    pub(super) refusals: GuestClipRefusals,
    /// SHA-256 of the image the guest is known to hold (clip_image.rs).
    pub(super) image: Option<[u8; 32]>,
    /// The PNG last synced either way, reused while it is `image`: a caption
    /// change needs no second transfer.
    pub(super) png: Option<host_pasteboard::PngImage>,
    /// The last refused guest image, so a re-polled one logs once.
    pub(super) refused_image: Option<GuestImageKey>,
}

impl ClipBaseline {
    /// The clipboard as one unit, either way: (text, image) is the pair last
    /// synced. Any other pair is written whole.
    pub(super) fn holds(&self, text: Option<&str>, image: Option<[u8; 32]>) -> bool {
        self.text.as_deref() == text && self.image == image
    }
}

impl AgentConsoleHarness {
    /// Collapse host pasteboard changes into one latest-wins ClipPush. Text the
    /// guest already holds is not re-sent, unless an older push is pending or
    /// the guest's text is unknown (rebaseline).
    pub(super) fn enqueue_host_clip_change(&mut self, now: Instant) {
        let Some(pasteboard) = self.pasteboard.as_ref() else {
            return;
        };
        let Some(clip) = std::iter::from_fn(|| pasteboard.try_changed_clip()).last() else {
            return;
        };
        // Images arrive only while the agent advertises IMAGE_PNG.
        match clip.image.filter(|_| self.clip_images.is_some()) {
            Some(HostImage::Png(png)) => return self.enqueue_host_image(clip.text, png, now),
            Some(too_large) => {
                self.skip_host_image(&too_large, now);
                if clip.text.is_none() {
                    self.drop_queued_host_pushes();
                    self.last_synced.rebaseline = true;
                    return;
                }
            }
            None => {}
        }
        let Some(text) = clip.text else {
            return;
        };
        let settled = !self.last_synced.rebaseline && !self.clip_push_pending();
        if settled && self.last_synced.holds(Some(&normalize_clip(&text)), None) {
            return;
        }
        if text.len() > self.clip_max_bytes {
            // Never pushed: the guest agent reads the wire with per-call
            // P/Invoke cost, so a huge CLIPSET line wedges the whole channel.
            // The Mac keeps its copy; queued older pushes are obsolete.
            println!(
                "BVAGENT CLIPSYNC skip bytes={} too-large t={}",
                text.len(),
                self.t_ms(now)
            );
            self.drop_queued_host_pushes();
            self.last_synced.rebaseline = true;
        } else {
            self.enqueue_clip_push(text);
        }
    }
    /// Coalesce host->guest clipboard pushes: a newer host value obsoletes any
    /// still-queued (unsent) ClipPush, so drop those before queuing this one.
    /// The in-flight push is already on the wire and is left alone.
    pub(super) fn enqueue_clip_push(&mut self, text: String) {
        self.drop_queued_host_pushes();
        self.queue.push_back(ServiceReq::ClipPush(text));
    }
    pub(super) fn clip_push_pending(&self) -> bool {
        self.any_pending(|req| matches!(req, ServiceReq::ClipPush(_) | ServiceReq::ClipImagePush(_)))
    }
    pub(super) fn handle_clip_poll_reply(&mut self, line: &str, now: Instant) {
        // CLIP <b64> answers CLIPGET; CLIPX adds the guest image (clip_image.rs).
        let (b64, image) = if let Some(b64) = line.strip_prefix("CLIP ") {
            (b64, None)
        } else if let Some(rest) = line.strip_prefix("CLIPX ") {
            let Ok((image, b64)) = parse_clipx(rest) else {
                self.complete_in_flight();
                return self.refuse_guest_image(GuestImageRefusal::Malformed, None, now);
            };
            (b64, Some(image))
        } else {
            if line.starts_with("ERR") {
                println!("BVAGENT CLIPSYNC guest->host {line}");
                self.complete_in_flight();
            }
            return;
        };
        self.complete_in_flight();
        // A reply that overtook a pending push predates it: the push
        // overwrites the guest, so the newer Mac copy wins.
        if self.clip_push_pending() {
            return;
        }
        if let Some(image) = image {
            return self.handle_guest_clip(b64, image, now);
        }
        let Some(text) = self.admit_guest_clip(b64, now) else {
            return;
        };
        // Adopt only genuinely-new content (normalized compare) so a value
        // we pushed host->guest doesn't bounce straight back.
        if let Some(normalized) = guest_clip_decision(&self.last_synced.text, &text) {
            self.publish_guest_text(normalized, now);
        }
    }
    /// The guest's text alone onto the Mac, replacing its text and image.
    pub(super) fn publish_guest_text(&mut self, text: String, now: Instant) {
        let bytes = text.len();
        self.last_synced.text = Some(text.clone());
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.set(text);
        }
        println!(
            "BVAGENT CLIPSYNC guest->host bytes={bytes} t={}",
            self.t_ms(now)
        );
    }
    pub(super) fn handle_clip_push_reply(&mut self, line: &str, guest_bytes: usize, now: Instant) {
        if line.starts_with("OK") {
            if let Some((ServiceReq::ClipPush(text), _)) = self.in_flight.take() {
                self.last_synced.text = Some(normalize_clip(&text));
                // CLIPSET replaces the whole guest clipboard.
                self.last_synced.image = None;
                self.last_synced.rebaseline = false;
            }
            println!(
                "BVAGENT CLIPSYNC host->guest bytes={guest_bytes} t={}",
                self.t_ms(now)
            );
        } else if line.starts_with("ERR") {
            println!("BVAGENT CLIPSYNC host->guest {line}");
            self.last_synced.rebaseline = true;
            self.complete_in_flight();
        }
    }
}
