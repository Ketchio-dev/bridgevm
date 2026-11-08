//! Clipboard images with an agent that answered CLIPCAPS with IMAGE_PNG and
//! the directory it keeps clipboard files in. PNG bytes move as files there,
//! through the shared folder's chunked PUT and GET verbs, never in a command
//! line:
//! - Mac to Windows: PUTBEG/PUTCHUNK/PUTEND `<dir>\host.png`, then
//!   `CLIPSETIMG <b64(path)> <b64(text)|->`, which sets the text (when the
//!   copy has one) and the image in one clipboard write. The text alone went
//!   first as a CLIPSET, as before.
//! - Windows to Mac: CLIPX replaces CLIPGET and adds the image's size and
//!   SHA-256. A new image is fetched with `GET <dir>\guest.png` (no other path)
//!   and reaches the Mac only as a PNG whose signature and IHDR are within the
//!   limits and whose SHA-256 matches. Nothing past the header is decoded.
//!
//! `ClipBaseline::image` is the image the guest is known to hold, so an image
//! never bounces back. Over a limit, an image stays where it was copied with
//! one log line, which the app shows as a notice. An agent without the marker
//! keeps text only, exactly as before.

use super::*;
use super::clip_html::GuestHtml;
use super::clip_sync::html_digest;
use host_pasteboard::{
    admit_png, parse_digest_hex, within_limits, HostImage, PngImage, PngRefusal,
    CLIP_IMAGE_MAX_BYTES, CLIP_IMAGE_MAX_SIDE,
};

#[cfg(test)]
#[path = "clip_image_tests.rs"]
mod tests;

pub(super) const CLIP_CAPS_IMAGE_MARKER: &str = "IMAGE_PNG";
/// CLIPCAPS PUSH asks for change notices; a reply with CLIPPUSH grants them.
pub(super) const CLIP_CAPS_PUSH_MARKER: &str = "CLIPPUSH";
/// The agent serves CLIPXH and CLIPSETH (rich text, clip_html.rs).
pub(super) const CLIP_CAPS_HTML_MARKER: &str = "CF_HTML";
/// Longest clipboard directory the agent may announce, in bytes.
const MAX_CLIP_DIR_BYTES: usize = 1024;

/// The agent's clipboard file directory, from its CLIPCAPS reply.
pub(super) struct ClipImages {
    pub(super) dir: String,
}

impl ClipImages {
    pub(super) fn host_path(&self) -> String {
        format!("{}\\host.png", self.dir)
    }
    pub(super) fn guest_path(&self) -> String {
        format!("{}\\guest.png", self.dir)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClipImagePhase {
    Beg,
    Chunk,
    End,
    Set,
}

/// A Mac image on its way to the guest clipboard.
#[derive(Clone)]
pub(super) struct ClipImagePush {
    pub(super) png: PngImage,
    /// The copy's text (LF), set together with the image.
    pub(super) text: Option<String>,
    pub(super) next_chunk: usize,
    pub(super) phase: ClipImagePhase,
}

impl ClipImagePush {
    pub(super) fn new(png: PngImage, text: Option<String>) -> Self {
        Self {
            png,
            text,
            next_chunk: 0,
            phase: ClipImagePhase::Beg,
        }
    }
    fn chunks(&self, chunk: usize) -> usize {
        self.png.bytes.len().div_ceil(chunk)
    }
}

/// Restart an interrupted image push from PUTBEG, which truncates the file.
pub(super) fn rewind_clip_image_push(mut req: ServiceReq) -> ServiceReq {
    if let ServiceReq::ClipImagePush(push) = &mut req {
        push.next_chunk = 0;
        push.phase = ClipImagePhase::Beg;
    }
    req
}

/// The image part of a CLIPX reply: `<w> <h> <bytes> <sha256|-> <b64(path)|->`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GuestImage {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bytes: usize,
    pub(super) digest: Option<[u8; 32]>,
    pub(super) path: Option<String>,
}

/// CLIPX `<w> <h> <bytes> <sha256|-> <b64(path)|-> <b64(text)>`: the image
/// (None without one) and the text's base64, or Err when malformed.
pub(super) fn parse_clipx(rest: &str) -> Result<(Option<GuestImage>, &str), ()> {
    let mut fields = rest.splitn(6, ' ');
    let mut next = || fields.next().ok_or(());
    let width = next()?.parse::<u32>().map_err(|_| ())?;
    let height = next()?.parse::<u32>().map_err(|_| ())?;
    let bytes = next()?.parse::<usize>().map_err(|_| ())?;
    let digest = match next()? {
        "-" => None,
        hex => Some(parse_digest_hex(hex).ok_or(())?),
    };
    let path = match next()? {
        "-" => None,
        b64 => Some(
            base64_decode(b64)
                .ok()
                .and_then(|path| String::from_utf8(path).ok())
                .ok_or(())?,
        ),
    };
    let text = next()?;
    let present = width > 0 || height > 0 || bytes > 0 || digest.is_some();
    let image = present.then_some(GuestImage {
        width,
        height,
        bytes,
        digest,
        path,
    });
    Ok((image, text))
}

/// The clipboard directory a CLIPCAPS reply announces with IMAGE_PNG. Unknown
/// markers are ignored.
pub(super) fn parse_clip_caps(rest: &str) -> Option<ClipImages> {
    let mut words = rest.split_whitespace();
    while let Some(word) = words.next() {
        if word != CLIP_CAPS_IMAGE_MARKER {
            continue;
        }
        let dir = base64_decode(words.next()?).ok()?;
        let dir = String::from_utf8(dir).ok()?;
        let dir = dir.trim_end_matches('\\');
        let usable = !dir.is_empty()
            && dir.len() <= MAX_CLIP_DIR_BYTES
            && !dir.chars().any(char::is_control);
        return usable.then(|| ClipImages {
            dir: dir.to_owned(),
        });
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GuestImageRefusal {
    TooLarge,
    UnexpectedPath,
    NotPng,
    Mismatch,
    GetError,
    Malformed,
}

impl GuestImageRefusal {
    fn reason(self) -> &'static str {
        match self {
            Self::TooLarge => "too-large",
            Self::UnexpectedPath => "unexpected-path",
            Self::NotPng => "not-png",
            Self::Mismatch => "mismatch",
            Self::GetError => "get-error",
            Self::Malformed => "malformed",
        }
    }
}

/// The bounded refusal record: sizes only, never image or path content. It
/// names no direction, so the app's run.log parser never reads it as a sync.
pub(super) fn guest_image_refusal_line(
    refusal: GuestImageRefusal,
    image: Option<&GuestImage>,
    copy: u64,
    t_ms: u128,
) -> String {
    let reason = refusal.reason();
    match image {
        Some(image) => format!(
            "BVAGENT CLIPSYNC refused guest-image {reason} bytes={} width={} height={} \
             limit-bytes={CLIP_IMAGE_MAX_BYTES} limit-side={CLIP_IMAGE_MAX_SIDE} copy={copy} t={t_ms}",
            image.bytes, image.width, image.height
        ),
        None => format!("BVAGENT CLIPSYNC refused guest-image {reason} copy={copy} t={t_ms}"),
    }
}

#[cfg(test)]
thread_local! {
    /// The lines clip_line printed on this test's thread.
    pub(super) static CLIP_LINES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Prints one clipboard line the app keys by copy number; tests read them back.
pub(super) fn clip_line(line: String) {
    #[cfg(test)]
    CLIP_LINES.with(|lines| lines.borrow_mut().push(line.clone()));
    println!("{line}");
}

/// A Mac image over a limit; the app shows it (and the refusal below with
/// `too-large`) as a notice (HvfClipboardImageNotice.swift).
pub(super) fn host_image_skip_line(
    bytes: usize,
    width: u32,
    height: u32,
    copy: u64,
    t_ms: u128,
) -> String {
    format!(
        "BVAGENT CLIPSYNC skip image too-large bytes={bytes} width={width} height={height} \
         limit-bytes={CLIP_IMAGE_MAX_BYTES} limit-side={CLIP_IMAGE_MAX_SIDE} copy={copy} t={t_ms}"
    )
}

impl AgentConsoleHarness {
    /// CLIPCAPS reply: IMAGE_PNG switches images on, CF_HTML (with it) rich
    /// text and CLIPPUSH change notices; a bare CLIPCAPS, or the unknown-token
    /// OUT of an agent that predates it, keeps a polled text-only sync.
    pub(super) fn handle_clip_caps_reply(&mut self, line: &str, now: Instant) {
        let (images, push, html) = if let Some(rest) = line.strip_prefix("CLIPCAPS") {
            let has = |marker| rest.split_whitespace().any(|word| word == marker);
            (parse_clip_caps(rest), has(CLIP_CAPS_PUSH_MARKER), has(CLIP_CAPS_HTML_MARKER))
        } else if line.starts_with("OUT ") || line.starts_with("ERR") {
            (None, false, false)
        } else {
            return;
        };
        self.complete_in_flight();
        if push != self.clip_push {
            println!(
                "BVAGENT CLIPSYNC guest push {} t={}",
                if push { "on" } else { "off" },
                self.t_ms(now)
            );
        }
        self.clip_push = push;
        let on = images.is_some();
        if !on {
            // Nothing queued for the old agent can be served; the baseline
            // image means nothing to a text-only clipboard sync.
            self.queue.retain(|req| {
                !matches!(
                    req,
                    ServiceReq::ClipImagePush(_) | ServiceReq::ClipImagePull { .. }
                )
            });
            self.last_synced.image = None;
            self.last_synced.png = None;
        }
        self.clip_images = images;
        let html = html && on;
        if !html {
            // A queued rich push becomes the plain one this agent can take.
            for req in self.queue.iter_mut() {
                if let ServiceReq::ClipPush(_, html) = req {
                    *html = None;
                }
            }
            self.last_synced.html = None;
        }
        if html != self.clip_html {
            println!("BVAGENT CLIPSYNC rich text {} t={}", if html { "on" } else { "off" }, self.t_ms(now));
        }
        self.clip_html = html;
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.set_images(on);
            pasteboard.set_rich_text(html);
        }
        println!(
            "BVAGENT CLIPSYNC images {} t={}",
            if on { "on" } else { "off" },
            self.t_ms(now)
        );
    }

    /// A new Mac copy the guest already holds: one line, so the app knows a
    /// later copy came (HvfClipboardImageNotice.swift).
    pub(super) fn host_copy_unchanged(&mut self, now: Instant) {
        clip_line(format!(
            "BVAGENT CLIPSYNC host->guest unchanged copy={} t={}",
            self.last_synced.host_copy,
            self.t_ms(now)
        ));
    }

    /// A Mac copy with an image: text first under the text rules, then the
    /// image with that text. Called only while images are on.
    pub(super) fn enqueue_host_image(&mut self, text: Option<String>, png: PngImage, now: Instant) {
        let text = text.filter(|text| {
            let fits = text.len() <= self.clip_max_bytes;
            if !fits {
                clip_line(format!(
                    "BVAGENT CLIPSYNC skip bytes={} too-large copy={} t={}",
                    text.len(),
                    self.last_synced.host_copy,
                    self.t_ms(now)
                ));
            }
            fits
        });
        let normalized = text.as_deref().map(normalize_clip);
        let settled = !self.last_synced.rebaseline && !self.clip_push_pending();
        if settled && self.last_synced.holds(normalized.as_deref(), Some(png.digest), None) {
            return self.host_copy_unchanged(now);
        }
        let text_held = settled && self.last_synced.text == normalized;
        self.drop_queued_host_pushes();
        if let Some(text) = text.as_ref().filter(|_| !text_held) {
            self.queue.push_back(ServiceReq::ClipPush(text.clone(), None));
        }
        self.queue
            .push_back(ServiceReq::ClipImagePush(ClipImagePush::new(png, text)));
    }

    /// A Mac image over a limit stays on the Mac, as an oversized text does.
    pub(super) fn skip_host_image(&mut self, image: &HostImage, now: Instant) {
        if let HostImage::TooLarge {
            bytes,
            width,
            height,
        } = image
        {
            let line = host_image_skip_line(*bytes, *width, *height, self.last_synced.host_copy, self.t_ms(now));
            clip_line(line);
        }
    }

    pub(super) fn drop_queued_host_pushes(&mut self) {
        self.queue.retain(|req| {
            !matches!(
                req,
                ServiceReq::ClipPush(..) | ServiceReq::ClipImagePush(_)
            )
        });
    }

    /// A CLIPX reply: the guest clipboard as one (text, image) pair, with
    /// CLIPXH a text's HTML too. Whenever it differs from what was last
    /// synced, the Mac gets exactly that: the text (with its HTML) at once, the
    /// image from the cache when its digest matches or else fetched and
    /// written with that text. An empty clipboard never clears the Mac.
    pub(super) fn handle_guest_clip(
        &mut self,
        b64: &str,
        image: Option<GuestImage>,
        html: GuestHtml<'_>,
        later: bool,
        echo: bool,
        now: Instant,
    ) {
        // Refusals first: each refused copy gets its line (and the app its
        // notice) before anything is learned or deduplicated, then counts as
        // none. Their dedup keys on the copy number: unchanged polls only.
        let admitted = self.admit_guest_text(b64, now);
        let copied_image = image.is_some();
        let image = image.filter(|image| match self.guest_image_refusal(image) {
            Some(refusal) => {
                self.refuse_guest_image(refusal, Some(image), now);
                false
            }
            None => true,
        });
        let text = admitted.as_deref().filter(|text| !text.is_empty()).map(normalize_clip);
        let digest = image.as_ref().and_then(|image| image.digest);
        // Rich text rides with a text copy, never with an image (v1).
        let html = if text.is_some() && !copied_image { html } else { GuestHtml::None };
        let html_held = html.b64().map(html_digest);
        if self.last_synced.rebaseline {
            // Learned whole, never published; a refused text keeps it pending.
            self.last_synced.rebaseline = admitted.is_none();
            self.last_synced.text = text;
            self.last_synced.image = digest;
            self.last_synced.html = html_held;
            return;
        }
        let held = self.last_synced.holds(text.as_deref(), digest, html_held);
        if held || (text.is_none() && !copied_image) {
            if echo && held {
                // A completed push back from the guest: still the Mac's copy.
                self.last_synced.guest_copy = self.last_synced.host_copy;
            } else if later {
                // A later copy that leaves the Mac as it is: one line, so the
                // app knows a later copy came (HvfClipboardImageNotice.swift).
                clip_line(format!(
                    "BVAGENT CLIPSYNC guest->host unchanged copy={} t={}",
                    self.last_synced.guest_copy,
                    self.t_ms(now)
                ));
            }
            return;
        }
        let fresh_text = self.last_synced.text != text || self.last_synced.html != html_held;
        let had_image = self.last_synced.image.is_some();
        // Recorded now, so a refused fetch is not retried every poll.
        self.last_synced.text = text.clone();
        self.last_synced.image = digest;
        self.last_synced.html = html_held;
        let (Some(image), Some(digest)) = (image, digest) else {
            // Text only: the guest's image is gone, or stays in Windows.
            if let Some(text) = text.filter(|_| fresh_text || had_image) {
                self.publish_guest_rich_text(text, &html, now);
            }
            return;
        };
        if let Some(png) = self.last_synced.png.take_if(|png| png.digest == digest) {
            return self.publish_guest_image(png, now);
        }
        // Text first, as for a text-only copy; the image follows with it.
        if let Some(text) = text.filter(|_| fresh_text) {
            self.publish_guest_text(text, now);
        }
        // The update's file stream or agent switch holds image transfers: a
        // queued fetch could find a later guest.png. The first poll after it
        // announces this image again, as after an agent restart.
        if self.agent_update_holds_images() {
            self.last_synced.image = None;
            return;
        }
        // Next on the wire, so no later CLIPX can replace guest.png first.
        self.queue.push_front(ServiceReq::ClipImagePull {
            digest,
            bytes: image.bytes,
        });
    }

    /// Why a guest image is not fetched: over a limit, malformed, or not the
    /// announced `<dir>\guest.png`.
    fn guest_image_refusal(&self, image: &GuestImage) -> Option<GuestImageRefusal> {
        let expected = self.clip_images.as_ref().map(ClipImages::guest_path);
        if !within_limits(image.bytes, image.width, image.height) {
            Some(GuestImageRefusal::TooLarge)
        } else if image.digest.is_none() || image.bytes == 0 {
            Some(GuestImageRefusal::Malformed)
        } else if expected.is_none() || image.path != expected {
            Some(GuestImageRefusal::UnexpectedPath)
        } else {
            None
        }
    }

    /// The guest's image onto the Mac with the pair's text, in one write. Its
    /// PNG stays cached for a later caption change.
    fn publish_guest_image(&mut self, png: PngImage, now: Instant) {
        let (len, width, height) = (png.bytes.len(), png.width, png.height);
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.set_image(png.clone(), self.last_synced.text.clone());
        }
        self.last_synced.png = Some(png);
        clip_line(format!(
            "BVAGENT CLIPSYNC guest->host image bytes={len} width={width} height={height} copy={} t={}",
            self.last_synced.guest_copy,
            self.t_ms(now)
        ));
    }

    pub(super) fn refuse_guest_image(
        &mut self,
        refusal: GuestImageRefusal,
        image: Option<&GuestImage>,
        now: Instant,
    ) {
        // One line per copy: its unchanged polls stay quiet.
        if self.last_synced.refused_copy == Some(self.last_synced.guest_copy) {
            return;
        }
        self.last_synced.refused_copy = Some(self.last_synced.guest_copy);
        self.last_synced.image_refusals += 1;
        let line = guest_image_refusal_line(refusal, image, self.last_synced.guest_copy, self.t_ms(now));
        clip_line(line);
    }

    /// PUT, then CLIPSETIMG, one reply at a time; the in-flight request keeps
    /// the wire for the whole push, as a shared-folder PUT does.
    pub(super) fn handle_clip_image_push_reply(
        &mut self,
        line: &str,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
    ) {
        // As a shared file's: PUTDATA chunks of 1 MiB for an agent that takes them.
        let (put_data, chunk) = (self.put_data, self.share_put_chunk_bytes());
        let Some((ServiceReq::ClipImagePush(push), sent_at)) = self.in_flight.as_mut() else {
            return;
        };
        let error = match push.phase {
            ClipImagePhase::Beg => "ERR PUTBEG",
            ClipImagePhase::Chunk => "ERR PUTCHUNK",
            ClipImagePhase::End => "ERR PUTEND",
            ClipImagePhase::Set => "ERR CLIPSETIMG",
        };
        let acked = match push.phase {
            ClipImagePhase::Beg => line == "OK PUTBEG",
            ClipImagePhase::Chunk => {
                line.strip_prefix("OK PUTCHUNK ")
                    .and_then(|seq| seq.parse::<usize>().ok())
                    == push.next_chunk.checked_sub(1)
            }
            ClipImagePhase::End => line.starts_with("PUTOK "),
            ClipImagePhase::Set => line == "OK CLIPSETIMG" || line.starts_with("OK CLIPSETIMG "),
        };
        if !acked {
            // An agent that lost the verb answers OUT; any other line is not ours.
            let set_refused = push.phase == ClipImagePhase::Set && line.starts_with("OUT ");
            if line.starts_with(error) || set_refused {
                println!("BVAGENT CLIPSYNC host->guest image-error {line}");
                self.last_synced.rebaseline = true;
                self.in_flight = None;
            }
            return;
        }
        *sent_at = now;
        let line_out = &mut self.service_line_scratch;
        line_out.clear();
        match push.phase {
            ClipImagePhase::Beg | ClipImagePhase::Chunk if push.next_chunk < push.chunks(chunk) => {
                let seq = push.next_chunk;
                let start = seq * chunk;
                let end = (start + chunk).min(push.png.bytes.len());
                put_chunk_line_into(line_out, put_data, seq, &push.png.bytes[start..end]);
                push.next_chunk += 1;
                push.phase = ClipImagePhase::Chunk;
            }
            ClipImagePhase::Beg | ClipImagePhase::Chunk => {
                let _ = writeln!(line_out, "PUTEND {}", push.chunks(chunk));
                push.phase = ClipImagePhase::End;
            }
            ClipImagePhase::End => {
                let Some(images) = self.clip_images.as_ref() else {
                    self.in_flight = None;
                    return;
                };
                line_out.push_str("CLIPSETIMG ");
                base64_encode_into(images.host_path().as_bytes(), line_out);
                match push.text.as_deref() {
                    Some(text) => {
                        line_out.push(' ');
                        to_guest_crlf_into(text, &mut self.clip_crlf_scratch);
                        base64_encode_into(self.clip_crlf_scratch.as_bytes(), line_out);
                    }
                    None => line_out.push_str(" -"),
                }
                line_out.push('\n');
                push.phase = ClipImagePhase::Set;
            }
            ClipImagePhase::Set => {
                let Some((ServiceReq::ClipImagePush(push), _)) = self.in_flight.take() else {
                    return;
                };
                self.last_synced.text = push.text.as_deref().map(normalize_clip);
                self.last_synced.image = Some(push.png.digest);
                self.last_synced.html = None;
                self.last_synced.rebaseline = false;
                self.last_synced.echo = true;
                clip_line(format!(
                    "BVAGENT CLIPSYNC host->guest image bytes={} width={} height={} copy={} t={}",
                    push.png.bytes.len(),
                    push.png.width,
                    push.png.height,
                    self.last_synced.host_copy,
                    self.t_ms(now)
                ));
                self.last_synced.png = Some(push.png);
                return;
            }
        }
        self.last_send = Some(now);
        send(self.service_line_scratch.as_bytes());
    }

    /// GETBEG/GETCHUNK accumulate as for any GET; GETEND checks the file and
    /// writes it to the Mac with the guest's current text.
    pub(super) fn handle_clip_image_pull_reply(&mut self, line: &str, now: Instant) {
        let error = line.starts_with("ERR");
        if !error && !line.starts_with("GETEND ") {
            return;
        }
        let Some((ServiceReq::ClipImagePull { digest, bytes }, _)) = self.in_flight.take() else {
            return;
        };
        let finished = self.take_finished_get();
        if error {
            self.refuse_guest_image(GuestImageRefusal::GetError, None, now);
            return;
        }
        let expected = self.clip_images.as_ref().map(ClipImages::guest_path);
        let png = match finished {
            Some(get) if Some(&get.path) == expected.as_ref()
                && get.total == bytes
                && get.bytes.len() == bytes =>
            {
                admit_png(get.bytes)
            }
            _ => Err(PngRefusal::NotPng),
        };
        let png = match png {
            Ok(png) if png.digest == digest => png,
            Ok(_) => return self.refuse_guest_image(GuestImageRefusal::Mismatch, None, now),
            Err(PngRefusal::TooLarge { .. }) => {
                return self.refuse_guest_image(GuestImageRefusal::TooLarge, None, now)
            }
            Err(PngRefusal::NotPng) => {
                return self.refuse_guest_image(GuestImageRefusal::NotPng, None, now)
            }
        };
        // A newer guest poll, a pending Mac copy or an app paste replaced it.
        if self.last_synced.image != Some(digest)
            || self.last_synced.rebaseline
            || self.clip_push_pending()
        {
            return;
        }
        self.publish_guest_image(png, now);
    }

    /// The wire line of a queued image request; false drops it (images off).
    pub(super) fn write_clip_image_line(&mut self, req: &ServiceReq) -> bool {
        let chunk = self.share_put_chunk_bytes();
        let Some(images) = self.clip_images.as_ref() else {
            return false;
        };
        let line = &mut self.service_line_scratch;
        match req {
            ServiceReq::ClipImagePush(push) => {
                debug_assert_eq!(push.phase, ClipImagePhase::Beg);
                line.push_str("PUTBEG ");
                base64_encode_into(images.host_path().as_bytes(), line);
                let _ = writeln!(line, " {} {}", push.png.bytes.len(), push.chunks(chunk));
            }
            ServiceReq::ClipImagePull { .. } => {
                // A new GET cannot inherit an earlier request's fragments.
                self.get_accum = None;
                line.push_str("GET ");
                base64_encode_into(images.guest_path().as_bytes(), line);
                line.push('\n');
            }
            _ => return false,
        }
        true
    }
}
