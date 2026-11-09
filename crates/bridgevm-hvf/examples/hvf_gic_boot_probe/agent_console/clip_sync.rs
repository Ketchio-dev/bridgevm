//! Clipboard auto-sync between the Mac pasteboard and the guest agent.
//!
//! The baseline is the text the guest is known to hold. It advances to a host
//! copy only on CLIPSET OK, so a copy the guest never received (oversized, ERR,
//! dropped by an agent restart, or overtaken by an earlier CLIPGET) cannot make
//! the guest's older text look like a new guest copy and revert the Mac. An
//! app paste through the guest clipboard re-learns it too (clip_paste.rs).

use super::*;
use super::clip_html::{cf_html_from_mac, mac_html_from_cf, parse_html_part, GuestHtml, CLIP_HTML_MAX_BYTES};
use sha2::{Digest, Sha256};

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
    /// SHA-256 of the base64 CF_HTML the guest is known to hold with `text`
    /// (clip_html.rs). The agent hands back the bytes it was given, and both
    /// ends write canonical base64, so the wire form is compared as it is.
    pub(super) html: Option<[u8; 32]>,
    /// SHA-256 of the CLIPXF files part the guest is known to hold, alone
    /// (clip_files.rs): published, refused or on its way; after a Finder copy
    /// went to the guest (clip_files_push.rs), that copy's digest until the
    /// guest lists it.
    pub(super) files: Option<[u8; 32]>,
    /// The PNG last synced either way, reused while it is `image`: a caption
    /// change needs no second transfer.
    pub(super) png: Option<host_pasteboard::PngImage>,
    /// The guest copy whose image refusal was logged: its unchanged polls stay quiet.
    pub(super) refused_copy: Option<u64>,
    /// Guest image refusals logged so far, one per refused copy.
    pub(super) image_refusals: u64,
    /// Copy numbers handed out so far, either way. The lines about one copy
    /// carry its number (` copy=N`), so the app can tell that copy's own text
    /// sync from a later copy (HvfClipboardImageNotice.swift).
    pub(super) copies: u64,
    /// The number of the latest Mac copy, and of the latest guest copy.
    pub(super) host_copy: u64,
    pub(super) guest_copy: u64,
    /// Fingerprint of the last guest poll reply: a re-poll keeps its copy number.
    pub(super) guest_reply: Option<u64>,
    /// A push just completed: the next changed guest reply that shows its pair
    /// is that Mac copy coming back, not a new guest copy.
    pub(super) echo: bool,
}

impl ClipBaseline {
    fn next_copy(&mut self) -> u64 {
        self.copies += 1;
        self.copies
    }

    /// The clipboard as one unit, either way: (text, image, html) is what was
    /// last synced, and no file drop. Anything else is written whole.
    pub(super) fn holds(&self, text: Option<&str>, image: Option<[u8; 32]>, html: Option<[u8; 32]>) -> bool {
        self.text.as_deref() == text && self.image == image && self.html == html && self.files.is_none()
    }
}

/// What the baseline keeps of a base64 CF_HTML.
pub(super) fn html_digest(b64: &str) -> [u8; 32] {
    Sha256::digest(b64.as_bytes()).into()
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
        self.last_synced.host_copy = self.last_synced.next_copy();
        // File copies arrive only while the agent advertises CLIPSETFILES. Over
        // a limit, or with a file that cannot be read, the copy goes as a plain
        // copy would: its names as text.
        match clip.files.filter(|_| self.clip_files_push) {
            Some(HostFiles::Read(files)) => return self.enqueue_host_files(clip.text, files, now),
            Some(HostFiles::TooLarge { entries, bytes }) => clip_line(format!(
                "BVAGENT CLIPSYNC skip host-files too-large files={entries} bytes={bytes} copy={} t={}",
                self.last_synced.host_copy,
                self.t_ms(now)
            )),
            Some(HostFiles::Unreadable { entries, bytes }) => clip_line(format!(
                "BVAGENT CLIPSYNC skip host-files unreadable files={entries} bytes={bytes} copy={} t={}",
                self.last_synced.host_copy,
                self.t_ms(now)
            )),
            None => {}
        }
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
        let html = clip.html.filter(|_| self.clip_html).and_then(|html| self.host_html(&html, now));
        let settled = !self.last_synced.rebaseline && !self.clip_push_pending();
        let held_html = html.as_deref().map(html_digest);
        if settled && self.last_synced.holds(Some(&normalize_clip(&text)), None, held_html) {
            return self.host_copy_unchanged(now);
        }
        if text.len() > self.clip_max_bytes {
            // Never pushed (over the 8 MiB the guest may send back, or the
            // operator's CLIPSYNC_MAX_KB): the Mac keeps its copy; queued
            // older pushes are obsolete.
            clip_line(format!(
                "BVAGENT CLIPSYNC skip bytes={} too-large copy={} t={}",
                text.len(),
                self.last_synced.host_copy,
                self.t_ms(now)
            ));
            self.drop_queued_host_pushes();
            self.last_synced.rebaseline = true;
        } else {
            self.enqueue_clip_push(text, html);
        }
    }
    /// The base64 CF_HTML for a Mac copy's `public.html`; over the limit the
    /// copy goes as text alone, with one line.
    fn host_html(&self, html: &str, now: Instant) -> Option<String> {
        let cf = cf_html_from_mac(html);
        if cf.len() > CLIP_HTML_MAX_BYTES {
            clip_line(format!(
                "BVAGENT CLIPSYNC skip html too-large bytes={} limit={CLIP_HTML_MAX_BYTES} copy={} t={}",
                cf.len(),
                self.last_synced.host_copy,
                self.t_ms(now)
            ));
            return None;
        }
        Some(base64_encode(&cf))
    }
    /// Coalesce host->guest clipboard pushes: a newer host value obsoletes any
    /// still-queued (unsent) ClipPush, so drop those before queuing this one.
    /// The in-flight push is already on the wire and is left alone.
    pub(super) fn enqueue_clip_push(&mut self, text: String, html: Option<String>) {
        self.drop_queued_host_pushes();
        self.queue.push_back(ServiceReq::ClipPush(text, html));
    }
    pub(super) fn clip_push_pending(&self) -> bool {
        self.any_pending(is_host_push)
    }
    pub(super) fn handle_clip_poll_reply(&mut self, line: &str, now: Instant) {
        // CLIP <b64> answers CLIPGET; CLIPX adds the guest image (clip_image.rs),
        // CLIPXH its HTML (clip_html.rs) and CLIPXF its files (clip_files.rs).
        let (b64, clipx, clipxh, clipxf) = if let Some(b64) = line.strip_prefix("CLIP ") {
            (b64, false, false, false)
        } else if let Some(rest) = line.strip_prefix("CLIPX ") {
            (rest, true, false, false)
        } else if let Some(rest) = line.strip_prefix("CLIPXH ") {
            (rest, true, true, false)
        } else if let Some(rest) = line.strip_prefix("CLIPXF ") {
            (rest, true, true, true)
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
        // The copy number, the one key of every dedup below: the clipboard's
        // image part and text whatever the verb, so a re-poll keeps it. A
        // changed reply is a new guest copy, unless a completed push comes back.
        let parsed = if clipx { parse_clipx(b64).ok() } else { Some((None, b64)) };
        let parsed = parsed.and_then(|(image, rest)| {
            let (html, after_html) = if clipxh { parse_html_part(rest)? } else { (GuestHtml::None, rest) };
            let (files, files_part, text) =
                if clipxf { parse_files_part(after_html)? } else { (GuestFiles::None, "", after_html) };
            Some((image, html, rest, after_html, files, files_part, text))
        });
        let reply = {
            let (part, html, files, text) = match &parsed {
                Some((_, html, rest, after_html, files, files_part, text)) if clipx => {
                    // Without HTML a CLIPXH reply keys like the CLIPX it extends,
                    // and without files a CLIPXF reply like the CLIPXH.
                    let html = if *html == GuestHtml::None { "" } else { rest.strip_suffix(*after_html).unwrap_or("") };
                    let files = if *files == GuestFiles::None { "" } else { *files_part };
                    (b64.strip_suffix(*rest).unwrap_or(b64), html, files, *text)
                }
                Some((.., text)) => ("0 0 0 - - ", "", "", *text),
                None => (b64, "", "", ""),
            };
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(part, &mut hasher);
            guest_clip_fingerprint(html, &mut hasher);
            // The whole files part: an edit deep in a long listing is a new copy, with its own
            // staging directory (only the copy number names it).
            if !files.is_empty() {
                std::hash::Hash::hash(files, &mut hasher);
            }
            guest_clip_fingerprint(text, &mut hasher)
        };
        // Only a later copy, never the first poll, counts for the app's notice.
        let later = self.last_synced.guest_reply.is_some_and(|last| last != reply);
        let changed = self.last_synced.guest_reply != Some(reply);
        let echo = changed && std::mem::take(&mut self.last_synced.echo);
        if changed {
            self.last_synced.guest_reply = Some(reply);
            self.last_synced.guest_copy = self.last_synced.next_copy();
        }
        if clipx {
            let Some((image, html, _, _, files, files_part, b64)) = parsed else {
                return self.refuse_guest_image(GuestImageRefusal::Malformed, None, now);
            };
            if changed && files == GuestFiles::Malformed {
                // Counts as no files: the copy's text and image still sync.
                clip_line(format!(
                    "BVAGENT CLIPSYNC refused guest-files malformed copy={} t={}",
                    self.last_synced.guest_copy,
                    self.t_ms(now)
                ));
            }
            if self.handle_guest_files(files, files_part, later, echo, now) {
                return;
            }
            return self.handle_guest_clip(b64, image, html, later, echo, now);
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
    /// The guest's text with its HTML onto the Mac, or the text alone when the
    /// HTML is over the limit or unusable (one line says which).
    pub(super) fn publish_guest_rich_text(&mut self, text: String, html: &GuestHtml, now: Instant) {
        let usable = match html {
            GuestHtml::Present(b64) => base64_decode(b64).ok().and_then(|cf| mac_html_from_cf(&cf)),
            _ => None,
        };
        let refusal = match html {
            GuestHtml::None => None,
            GuestHtml::TooLarge(bytes) => Some(format!("too-large bytes={bytes} limit={CLIP_HTML_MAX_BYTES}")),
            GuestHtml::Present(_) => usable.is_none().then(|| "malformed".to_owned()),
        };
        if let Some(reason) = refusal {
            clip_line(format!(
                "BVAGENT CLIPSYNC refused guest-html {reason} copy={} t={}",
                self.last_synced.guest_copy,
                self.t_ms(now)
            ));
        }
        let Some(mac_html) = usable else {
            return self.publish_guest_text(text, now);
        };
        let (bytes, html_bytes) = (text.len(), mac_html.len());
        self.last_synced.text = Some(text.clone());
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.set_text_html(text, mac_html);
        }
        clip_line(format!(
            "BVAGENT CLIPSYNC guest->host bytes={bytes} html={html_bytes} copy={} t={}",
            self.last_synced.guest_copy,
            self.t_ms(now)
        ));
    }
    /// The guest's text alone onto the Mac, replacing its text and image.
    pub(super) fn publish_guest_text(&mut self, text: String, now: Instant) {
        let bytes = text.len();
        self.last_synced.text = Some(text.clone());
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.set(text);
        }
        clip_line(format!(
            "BVAGENT CLIPSYNC guest->host bytes={bytes} copy={} t={}",
            self.last_synced.guest_copy,
            self.t_ms(now)
        ));
    }
    pub(super) fn handle_clip_push_reply(&mut self, line: &str, guest_bytes: usize, now: Instant) {
        if line.starts_with("OK") {
            let mut html_note = String::new();
            if let Some((ServiceReq::ClipPush(text, html), _)) = self.in_flight.take() {
                self.last_synced.text = Some(normalize_clip(&text));
                // CLIPSET and CLIPSETH replace the whole guest clipboard.
                self.last_synced.image = None;
                self.last_synced.files = None;
                self.last_synced.html = html.as_deref().map(html_digest);
                self.last_synced.echo = true;
                self.last_synced.rebaseline = false;
                if let Some(html) = html {
                    let padding = html.bytes().rev().take_while(|byte| *byte == b'=').count();
                    html_note = format!(" html={}", html.len() / 4 * 3 - padding);
                }
            }
            clip_line(format!(
                "BVAGENT CLIPSYNC host->guest bytes={guest_bytes}{html_note} copy={} t={}",
                self.last_synced.host_copy,
                self.t_ms(now)
            ));
        } else if line.starts_with("ERR") || line.starts_with("OUT ") {
            // OUT: an agent that does not know CLIPSETH (see handle_clip_caps_reply).
            println!("BVAGENT CLIPSYNC host->guest {line}");
            self.last_synced.rebaseline = true;
            self.complete_in_flight();
        }
    }
}
