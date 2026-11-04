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
    /// The image part CLIPX reports instead, as the agent describes an image
    /// it does not write.
    described: Option<String>,
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
            described: None,
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
                    _ if self.described.is_some() => self.described.clone().unwrap(),
                    // As bvagent.ps1: over the side limit it is never encoded or hashed.
                    Some(bytes)
                        if host_pasteboard::png_size(bytes)
                            .is_some_and(|(w, h)| w.max(h) > CLIP_IMAGE_MAX_SIDE) =>
                    {
                        let (w, h) = host_pasteboard::png_size(bytes).unwrap();
                        format!("{w} {h} 0 - -")
                    }
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
        let mut rig = Self::unsettled(images);
        rig.settle();
        rig
    }
    /// The same before its first clip interval, so both clipboards can be seeded.
    fn unsettled(images: bool) -> Self {
        let (pasteboard, worker) = HostPasteboard::test_pair();
        let mut h = harness();
        h.service = true;
        h.clipsync = true;
        h.commands.clear();
        h.pasteboard = Some(pasteboard);
        let now = Instant::now();
        assert!(h.next_command_line(0, now).is_none());
        Self {
            h,
            worker,
            mac: Mac::default(),
            guest: Guest::new(images),
            wire: Vec::new(),
            now,
        }
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

/// A clipboard as one unit: its text and its image (a `pair_image` id).
type Pair = (Option<&'static str>, Option<u8>);
type Held = (Option<String>, Option<u8>);

fn pair_image(id: u8) -> Vec<u8> {
    png(8, 8 + u32::from(id), 200, id)
}

/// 0 for an image that is neither test image.
fn image_id(bytes: &[u8]) -> u8 {
    (1..=2).find(|id| pair_image(*id) == bytes).unwrap_or(0)
}

fn held((text, image): Pair) -> Held {
    (text.map(str::to_owned), image)
}

fn mac_held(mac: &Mac) -> Held {
    let image = match &mac.image {
        Some(PasteboardImage::Png(bytes)) => Some(image_id(bytes)),
        Some(_) => Some(0),
        None => None,
    };
    (mac.text.clone(), image)
}

fn guest_held(guest: &Guest) -> Held {
    let text = (!guest.text.is_empty()).then(|| guest.text.clone());
    (text, guest.image.as_deref().map(image_id))
}

/// Either side's (text, image) pair lands on the other side whole whenever it
/// changes, and never comes back.
#[test]
fn every_clipboard_pair_change_lands_whole_on_the_other_side() {
    const STEPS: [(&str, Pair, Pair); 7] = [
        ("none -> A", (None, None), (Some("A"), None)),
        ("A -> A+X", (Some("A"), None), (Some("A"), Some(1))),
        ("A+X -> B+X", (Some("A"), Some(1)), (Some("B"), Some(1))),
        ("B+X -> X", (Some("B"), Some(1)), (None, Some(1))),
        ("X -> Y", (None, Some(1)), (None, Some(2))),
        ("Y -> A", (None, Some(2)), (Some("A"), None)),
        ("A+X -> A", (Some("A"), Some(1)), (Some("A"), None)),
    ];
    let mut failures = Vec::new();
    for (name, from, to) in STEPS {
        // Windows copies `from`, then `to`: the Mac gets each, nothing goes back.
        let mut rig = Rig::new(true);
        rig.verbs();
        for pair in [from, to] {
            rig.guest.text = pair.0.unwrap_or("").to_owned();
            rig.guest.image = pair.1.map(pair_image);
            for _ in 0..3 {
                rig.settle();
            }
            let pushed_back = rig.verbs().iter().any(|verb| verb != "GET");
            if mac_held(&rig.mac) != held(pair) || pushed_back {
                failures.push(format!("guest->mac {name}: at {pair:?} the Mac holds {:?}", mac_held(&rig.mac)));
                break;
            }
        }
        // The Mac copies `from`, then `to`: Windows gets each, nothing comes back.
        let mut rig = Rig::new(true);
        for pair in [from, to] {
            if pair != (None, None) {
                rig.mac.copy(pair.0, pair.1.map(|id| PasteboardImage::Png(pair_image(id))));
            }
            for _ in 0..3 {
                rig.settle();
            }
            let fetched_back = rig.verbs().iter().any(|verb| verb == "GET");
            if guest_held(&rig.guest) != held(pair) || fetched_back || !rig.mac.writes.is_empty() {
                failures.push(format!("mac->guest {name}: at {pair:?} Windows holds {:?}", guest_held(&rig.guest)));
                break;
            }
        }
    }
    // Startup: a Mac image wins over the guest text polled with it ...
    let mut rig = Rig::unsettled(true);
    rig.mac.copy(None, Some(PasteboardImage::Png(pair_image(1))));
    rig.guest.text = "W".into();
    for _ in 0..3 {
        rig.settle();
    }
    if guest_held(&rig.guest) != (None, Some(1)) || !rig.mac.writes.is_empty() {
        failures.push(format!("startup mac image: Windows holds {:?}", guest_held(&rig.guest)));
    }
    // ... and a guest pair reaches an empty Mac.
    let mut rig = Rig::unsettled(true);
    rig.guest.text = "A".into();
    rig.guest.image = Some(pair_image(1));
    for _ in 0..3 {
        rig.settle();
    }
    if mac_held(&rig.mac) != (Some("A".into()), Some(1)) {
        failures.push(format!("startup guest pair: the Mac holds {:?}", mac_held(&rig.mac)));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_new_caption_reuses_the_image_already_synced() {
    let mut rig = Rig::new(true);
    rig.verbs();
    for text in ["A", "B", ""] {
        rig.guest.text = text.into();
        rig.guest.image = Some(pair_image(1));
        rig.settle();
    }
    assert_eq!(rig.verbs(), verbs(&["GET"]), "the image is fetched once");
    assert_eq!(mac_held(&rig.mac), (None, Some(1)));
    // A Mac image that Windows re-captions comes back from the cache too.
    rig.mac.copy(Some("C"), Some(PasteboardImage::Png(pair_image(2))));
    rig.settle();
    rig.verbs();
    rig.guest.text = "D".into();
    rig.settle();
    assert!(rig.verbs().is_empty(), "no GET for the image the Mac pushed");
    assert_eq!(mac_held(&rig.mac), (Some("D".into()), Some(2)));
}

/// Each refused guest copy gets its line (the app's notice) once, also when
/// the pair it leaves for the Mac looks unchanged.
#[test]
fn every_refused_guest_copy_is_reported_once() {
    let over_bytes = format!("4000 3000 {} - -", CLIP_IMAGE_MAX_BYTES + 1);
    let over_side = format!("{} 2 0 - -", CLIP_IMAGE_MAX_SIDE + 1);
    // (case, text the guest held first, then the refused copy: its text and image part)
    let cases: [(&str, Option<&str>, &str, String); 6] = [
        ("over the byte limit, no text, empty baseline", None, "", over_bytes.clone()),
        ("over the side limit, no text, empty baseline", None, "", over_side),
        ("over the byte limit, unchanged text", Some("A"), "A", over_bytes.clone()),
        ("no digest, empty baseline", None, "", "3 3 33 - -".into()),
        ("the synced image at another path", None, "", String::new()),
        ("over the byte limit while re-learning", None, "", over_bytes.clone()),
    ];
    let mut failures = Vec::new();
    for (name, first, text, described) in cases {
        let mut rig = Rig::new(true);
        if let Some(first) = first {
            rig.guest.text = first.into();
        }
        if described.is_empty() {
            rig.guest.image = Some(pair_image(1));
        }
        rig.settle();
        rig.verbs();
        let writes = rig.mac.writes.len();
        rig.guest.text = text.into();
        if described.is_empty() {
            rig.guest.announce = Some(format!("{DIR}\\other.png"));
        } else {
            rig.guest.described = Some(described);
        }
        if name.ends_with("re-learning") {
            rig.h.last_synced.rebaseline = true;
        }
        for _ in 0..3 {
            rig.settle();
        }
        let refusals = rig.h.last_synced.image_refusals;
        let fetched = rig.verbs().iter().any(|verb| verb == "GET");
        if refusals != 1 || fetched || rig.mac.writes.len() != writes {
            failures.push(format!("{name}: {refusals} refusal lines, fetched={fetched}"));
        }
    }
    // The same refused image copied again after another copy: a line each time.
    let mut rig = Rig::new(true);
    for described in [Some(over_bytes.clone()), None, Some(over_bytes)] {
        rig.guest.text = if described.is_some() { String::new() } else { "B".into() };
        rig.guest.described = described;
        rig.settle();
        rig.settle();
    }
    if rig.h.last_synced.image_refusals != 2 {
        failures.push(format!("copied again: {} refusal lines", rig.h.last_synced.image_refusals));
    }
    assert!(failures.is_empty(), "{failures:#?}");
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

/// The lines about one copy share its number, a re-poll keeps it, and a later
/// copy either way gets a larger one: the app's notice is keyed by it.
#[test]
fn copy_numbers_follow_copies_not_polls() {
    let mut rig = Rig::new(true);
    rig.guest.text = "A".into();
    rig.guest.described = Some(format!("4000 3000 {} - -", CLIP_IMAGE_MAX_BYTES + 1));
    rig.settle();
    let refused = rig.h.last_synced.guest_copy;
    assert_eq!((rig.h.last_synced.image_refusals, rig.mac.text.as_deref()), (1, Some("A")));
    for _ in 0..3 {
        rig.settle();
    }
    assert_eq!(rig.h.last_synced.guest_copy, refused, "re-polls keep the number");
    rig.mac.copy(Some("M"), None);
    rig.settle();
    let mac = rig.h.last_synced.host_copy;
    assert!(mac > refused);
    rig.guest.text = "B".into();
    rig.guest.described = None;
    rig.settle();
    assert!(rig.h.last_synced.guest_copy > mac);
}

#[test]
fn notice_lines_keep_the_prefixes_the_app_matches() {
    // HvfClipboardImageNotice.swift matches these prefixes exactly.
    assert!(host_image_skip_line(40_000_000, 9000, 9000, 3, 812)
        .starts_with("BVAGENT CLIPSYNC skip image too-large bytes=40000000 width=9000 height=9000 "));
    let image = GuestImage { width: 20000, height: 300, bytes: 0, digest: None, path: None };
    let refused = guest_image_refusal_line(GuestImageRefusal::TooLarge, Some(&image), 4, 913);
    assert_eq!(
        refused,
        "BVAGENT CLIPSYNC refused guest-image too-large bytes=0 width=20000 height=300 \
         limit-bytes=33554432 limit-side=16384 copy=4 t=913"
    );
}
