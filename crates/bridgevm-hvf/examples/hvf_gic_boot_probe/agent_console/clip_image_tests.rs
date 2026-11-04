//! Clipboard images through the resident service loop, against an in-memory
//! Mac pasteboard and a guest that answers the way `bvagent.ps1` does.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use crate::agent_console::host_pasteboard::{
    HostClip, PasteboardBackend, PasteboardImage, PasteboardWorker,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const DIR: &str = r"C:\Users\u\AppData\Local\Temp\BridgeVM-clip";

/// A PNG header for `width` x `height` followed by `fill` bytes of `seed`,
/// so images differ in size and digest; nothing here decodes past IHDR.
fn png(width: u32, height: u32, fill: usize, seed: u8) -> Vec<u8> {
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13];
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    png.resize(png.len() + fill, seed);
    png
}

#[derive(Default)]
struct Mac {
    count: i64,
    text: Option<String>,
    image: Option<PasteboardImage>,
    image_reads: usize,
    writes: Vec<(Option<String>, Option<Vec<u8>>)>,
}

impl Mac {
    fn copy(&mut self, text: Option<&str>, image: Option<PasteboardImage>) {
        self.count += 1;
        self.text = text.map(str::to_owned);
        self.image = image;
    }
}

impl PasteboardBackend for Mac {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.copy(Some(text), None);
        self.writes.push((Some(text.to_owned()), None));
        true
    }
    fn get_image(&mut self) -> Option<PasteboardImage> {
        self.image_reads += 1;
        self.image.clone()
    }
    fn set_image(&mut self, png: &[u8], text: Option<&str>) -> bool {
        self.copy(text, Some(PasteboardImage::Png(png.to_vec())));
        self.writes.push((text.map(str::to_owned), Some(png.to_vec())));
        true
    }
}

/// The guest side of the wire, as bvagent.ps1 answers it.
struct Guest {
    images: bool,
    text: String,
    image: Option<Vec<u8>>,
    files: HashMap<String, Vec<u8>>,
    put: Option<(String, Vec<u8>)>,
    /// Path the next CLIPX announces (a misbehaving agent may differ).
    announce: Option<String>,
    /// Bytes the next GET serves instead of the file.
    serve: Option<Vec<u8>>,
}

impl Guest {
    fn new(images: bool) -> Self {
        Self {
            images,
            text: String::new(),
            image: None,
            files: HashMap::new(),
            put: None,
            announce: None,
            serve: None,
        }
    }
    fn b64_text(s: &str) -> String {
        String::from_utf8(base64_decode(s).unwrap()).unwrap()
    }
    fn answer(&mut self, line: &str) -> Vec<String> {
        let line = line.trim_end();
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let reply = match verb {
            "CLIPCAPS" if self.images => format!("CLIPCAPS IMAGE_PNG {}", base64_encode(DIR.as_bytes())),
            "CLIPGET" => format!("CLIP {}", base64_encode(self.text.as_bytes())),
            "CLIPX" if self.images => {
                let guest_path = format!("{DIR}\\guest.png");
                let descriptor = match &self.image {
                    Some(bytes) => {
                        self.files.insert(guest_path.clone(), bytes.clone());
                        let (w, h) = host_pasteboard::png_size(bytes).unwrap();
                        let path = self.announce.clone().unwrap_or(guest_path);
                        let sha: [u8; 32] = Sha256::digest(bytes).into();
                        let hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
                        format!("{w} {h} {} {hex} {}", bytes.len(), base64_encode(path.as_bytes()))
                    }
                    None => "0 0 0 - -".to_owned(),
                };
                format!("CLIPX {descriptor} {}", base64_encode(self.text.replace('\n', "\r\n").as_bytes()))
            }
            "CLIPSET" => {
                self.text = Self::b64_text(arg).replace("\r\n", "\n");
                self.image = None;
                "OK CLIPSET".to_owned()
            }
            "PUTBEG" => {
                let path = Self::b64_text(arg.split(' ').next().unwrap());
                self.put = Some((path, Vec::new()));
                "OK PUTBEG".to_owned()
            }
            "PUTCHUNK" => {
                let (seq, b64) = arg.split_once(' ').unwrap();
                self.put.as_mut().unwrap().1.extend(base64_decode(b64).unwrap());
                format!("OK PUTCHUNK {seq}")
            }
            "PUTEND" => {
                let (path, bytes) = self.put.take().unwrap();
                let reply = format!("PUTOK {} {}", base64_encode(path.as_bytes()), bytes.len());
                self.files.insert(path, bytes);
                reply
            }
            "CLIPSETIMG" if self.images => {
                let (path, text) = arg.split_once(' ').unwrap();
                let bytes = self.files.remove(&Self::b64_text(path)).unwrap();
                self.text = if text == "-" { String::new() } else { Self::b64_text(text).replace("\r\n", "\n") };
                self.image = Some(bytes);
                "OK CLIPSETIMG 1 1".to_owned()
            }
            "GET" => {
                let path = Self::b64_text(arg);
                let bytes = self.serve.take().unwrap_or_else(|| self.files[&path].clone());
                let chunks: Vec<&[u8]> = bytes.chunks(24 * 1024).collect();
                let mut lines = vec![format!("GETBEG {arg} {} {}", bytes.len(), chunks.len())];
                for (seq, chunk) in chunks.iter().enumerate() {
                    lines.push(format!("GETCHUNK {seq} {}", base64_encode(chunk)));
                }
                lines.push(format!("GETEND {}", chunks.len()));
                return lines;
            }
            _ => format!("OUT 255 {}", base64_encode(format!("unknown token: {verb}").as_bytes())),
        };
        vec![reply]
    }
}

struct Rig {
    h: AgentConsoleHarness,
    worker: PasteboardWorker,
    mac: Mac,
    guest: Guest,
    wire: Vec<String>,
    now: Instant,
}

impl Rig {
    /// A resident service right after `BVAGENT SERVICE start`.
    fn new(images: bool) -> Self {
        let (pasteboard, worker) = HostPasteboard::test_pair();
        let mut h = harness();
        h.service = true;
        h.clipsync = true;
        h.commands.clear();
        h.pasteboard = Some(pasteboard);
        let now = Instant::now();
        assert!(h.next_command_line(0, now).is_none());
        let mut rig = Self {
            h,
            worker,
            mac: Mac::default(),
            guest: Guest::new(images),
            wire: Vec::new(),
            now,
        };
        rig.settle();
        rig
    }
    /// One clip interval: the Mac poll, a service pass, every reply in
    /// lockstep, then the Mac applies queued writes.
    fn settle(&mut self) {
        self.now += Duration::from_secs(1);
        assert!(self.worker.poll(&mut self.mac));
        self.h.service_enqueue(self.now);
        loop {
            let mut sent = Vec::new();
            self.h
                .service_pump_with_sender(self.now, &mut |bytes| sent.push(bytes.to_vec()));
            if sent.is_empty() {
                break;
            }
            let mut wire: VecDeque<Vec<u8>> = sent.into();
            while let Some(bytes) = wire.pop_front() {
                let line = String::from_utf8(bytes).unwrap();
                self.wire.push(line.trim_end().to_owned());
                for reply in self.guest.answer(&line) {
                    let mut more = Vec::new();
                    self.h.handle_service_reply_with_sender(&reply, self.now, &mut |bytes| {
                        more.push(bytes.to_vec())
                    });
                    wire.extend(more);
                }
            }
        }
        assert!(self.h.in_flight.is_none());
        assert!(self.worker.apply_pending(&mut self.mac));
    }
    fn verbs(&mut self) -> Vec<String> {
        let verbs = self
            .wire
            .iter()
            .map(|line| line.split(' ').next().unwrap().to_owned())
            .filter(|verb| verb != "CLIPX" && verb != "CLIPGET")
            .collect();
        self.wire.clear();
        verbs
    }
}

fn verbs(list: &[&str]) -> Vec<String> {
    list.iter().map(|verb| verb.to_string()).collect()
}

#[test]
fn clipx_and_clipcaps_parse_their_fields() {
    let digest = "ab".repeat(32);
    let path = base64_encode(br"C:\T\BridgeVM-clip\guest.png");
    let rest = format!("3 2 33 {digest} {path} {}", base64_encode(b"hi"));
    let (image, text) = parse_clipx(&rest).expect("well formed");
    let image = image.expect("image");
    assert_eq!((image.width, image.height, image.bytes), (3, 2, 33));
    assert_eq!(image.path.as_deref(), Some(r"C:\T\BridgeVM-clip\guest.png"));
    assert_eq!(text, base64_encode(b"hi"));
    assert_eq!(parse_clipx("0 0 0 - - "), Ok((None, "")));
    assert!(parse_clipx("0 0 0 -").is_err());
    assert!(parse_clipx("x 0 0 - - ").is_err());
    let dir = base64_encode(br"C:\T\BridgeVM-clip\");
    let caps = parse_clip_caps(&format!(" FUTURE_MARKER IMAGE_PNG {dir}")).expect("marker");
    assert_eq!(caps.guest_path(), r"C:\T\BridgeVM-clip\guest.png");
    assert!(parse_clip_caps("").is_none());
    assert!(parse_clip_caps(&format!(" IMAGE_PNG {}", base64_encode(b"a\nb"))).is_none());
}

#[test]
fn old_agent_keeps_text_only_and_the_mac_image_is_never_read() {
    let mut rig = Rig::new(false);
    assert!(rig.h.clip_images.is_none());
    rig.mac.copy(Some("caption"), Some(PasteboardImage::Png(png(4, 4, 10, 1))));
    rig.settle();
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS", "CLIPSET"]));
    assert_eq!(rig.guest.text, "caption");
    assert_eq!(rig.mac.image_reads, 0, "no image read without the marker");
    rig.guest.text = "guest".into();
    rig.settle();
    assert_eq!(rig.mac.writes, vec![(Some("guest".to_owned()), None)]);
}

#[test]
fn mac_image_reaches_the_guest_after_its_text() {
    let mut rig = Rig::new(true);
    assert!(rig.h.clip_images.is_some());
    let image = png(64, 32, 60 * 1024, 7);
    rig.mac.copy(Some("caption"), Some(PasteboardImage::Png(image.clone())));
    rig.settle();
    assert_eq!(
        rig.verbs(),
        verbs(&["CLIPCAPS", "CLIPSET", "PUTBEG", "PUTCHUNK", "PUTCHUNK", "PUTCHUNK", "PUTEND", "CLIPSETIMG"])
    );
    assert_eq!(rig.guest.image.as_deref(), Some(image.as_slice()));
    assert_eq!(rig.guest.text, "caption");
    // Image only: the guest's text goes with it.
    let second = png(8, 8, 100, 9);
    rig.mac.copy(None, Some(PasteboardImage::Png(second.clone())));
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["PUTBEG", "PUTCHUNK", "PUTEND", "CLIPSETIMG"]));
    assert_eq!(rig.guest.image.as_deref(), Some(second.as_slice()));
    assert_eq!(rig.guest.text, "");
}

#[test]
fn pushed_image_never_bounces_back() {
    let mut rig = Rig::new(true);
    rig.mac.copy(None, Some(PasteboardImage::Png(png(5, 5, 500, 3))));
    rig.settle();
    rig.verbs();
    for _ in 0..3 {
        rig.settle();
    }
    assert!(rig.verbs().is_empty(), "no GET, no second push");
    assert!(rig.mac.writes.is_empty(), "the guest's copy of the Mac image is not written back");
}

#[test]
fn guest_image_is_checked_then_written_once_with_its_text() {
    let mut rig = Rig::new(true);
    let image = png(1920, 1080, 30 * 1024, 5);
    rig.guest.text = "cells\n".into();
    rig.guest.image = Some(image.clone());
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS", "GET"]));
    assert_eq!(
        rig.mac.writes,
        vec![
            (Some("cells\n".to_owned()), None),
            (Some("cells\n".to_owned()), Some(image)),
        ],
        "text first, then text and image in one write"
    );
    for _ in 0..3 {
        rig.settle();
    }
    assert!(rig.verbs().is_empty(), "neither fetched again nor pushed back");
    assert_eq!(rig.mac.writes.len(), 2);
}

#[test]
fn guest_image_is_refused_for_path_size_signature_and_digest() {
    // A path other than <dir>\guest.png is never fetched.
    let mut rig = Rig::new(true);
    rig.guest.announce = Some(r"C:\Windows\System32\config\SAM".into());
    rig.guest.image = Some(png(3, 3, 10, 1));
    rig.settle();
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS"]));
    // Over the side limit: never fetched.
    let mut rig = Rig::new(true);
    rig.guest.image = Some(png(CLIP_IMAGE_MAX_SIDE + 1, 2, 10, 1));
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS"]));
    // Served bytes that are no PNG, or another PNG: fetched, never written.
    for served in [b"MZ not a png at all, but long enough to fill an IHDR".to_vec(), png(3, 3, 10, 2)] {
        let mut rig = Rig::new(true);
        rig.guest.image = Some(png(3, 3, 10, 1));
        rig.guest.serve = Some(served);
        rig.settle();
        rig.settle();
        assert_eq!(rig.verbs(), verbs(&["CLIPCAPS", "GET"]), "refused once, not refetched");
        assert!(rig.mac.writes.is_empty());
    }
}

#[test]
fn oversized_mac_image_stays_on_the_mac_while_its_text_syncs() {
    let mut rig = Rig::new(true);
    let huge = PasteboardImage::TooLarge { bytes: CLIP_IMAGE_MAX_BYTES + 1, width: 9000, height: 9000 };
    rig.mac.copy(Some("caption"), Some(huge.clone()));
    rig.settle();
    assert_eq!(rig.verbs(), verbs(&["CLIPCAPS", "CLIPSET"]));
    assert_eq!(rig.guest.text, "caption");
    rig.mac.copy(None, Some(huge));
    rig.settle();
    rig.settle();
    assert!(rig.verbs().is_empty());
    assert_eq!(rig.guest.text, "caption", "the guest keeps its copy");
    assert!(rig.mac.writes.is_empty(), "nor is the Mac's image replaced");
}

#[test]
fn agent_restart_mid_push_resends_the_image_from_putbeg() {
    let mut rig = Rig::new(true);
    rig.verbs();
    let image = png(10, 10, 50 * 1024, 4);
    assert!(rig.worker.report_clip(HostClip {
        text: None,
        image: Some(HostImage::Png(host_pasteboard::admit_png(image.clone()).unwrap())),
    }));
    rig.h.service_enqueue(rig.now);
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| sent.push(bytes.to_vec()));
    assert!(String::from_utf8_lossy(&sent[0]).starts_with("PUTBEG "));
    rig.h.handle_service_reply_with_sender("OK PUTBEG", rig.now, &mut |_| {});
    rig.h.handle_service_reply_with_sender("READY GUEST v3-share2", rig.now, &mut |_| {});
    assert!(matches!(rig.h.queue.front(), Some(ServiceReq::ClipCaps)));
    rig.settle();
    assert_eq!(rig.guest.image.as_deref(), Some(image.as_slice()));
    assert!(rig.verbs().starts_with(&verbs(&["CLIPCAPS", "PUTBEG"])));
}

#[test]
fn notice_lines_keep_the_prefixes_the_app_matches() {
    // HvfClipboardImageNotice.swift matches these prefixes exactly.
    assert!(host_image_skip_line(40_000_000, 9000, 9000, 812)
        .starts_with("BVAGENT CLIPSYNC skip image too-large bytes=40000000 width=9000 height=9000 "));
    let image = GuestImage { width: 20000, height: 300, bytes: 0, digest: None, path: None };
    let refused = guest_image_refusal_line(GuestImageRefusal::TooLarge, Some(&image), 913);
    assert_eq!(
        refused,
        "BVAGENT CLIPSYNC refused guest-image too-large bytes=0 width=20000 height=300 \
         limit-bytes=33554432 limit-side=16384 t=913"
    );
}
