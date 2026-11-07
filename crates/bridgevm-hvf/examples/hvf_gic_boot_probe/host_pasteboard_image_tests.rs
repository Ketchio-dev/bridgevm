//! The PNG header check, and NSPasteboard image reads and writes on a private
//! uniquely named pasteboard (never the user's clipboard).

use super::super::{HostClip, NsPasteboardBackend, PasteboardBackend, Poller};
use super::*;

/// A PNG prefix: signature and IHDR for `width` x `height`; enough for the
/// header check, which never reads past IHDR.
pub(crate) fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&[0; 4]); // CRC, not checked
    png
}

#[test]
fn png_size_reads_ihdr_and_refuses_everything_else() {
    assert_eq!(png_size(&png_header(640, 480)), Some((640, 480)));
    let mut bad_signature = png_header(1, 1);
    bad_signature[1] = b'J';
    assert_eq!(png_size(&bad_signature), None);
    let mut not_ihdr = png_header(1, 1);
    not_ihdr[12..16].copy_from_slice(b"IDAT");
    assert_eq!(png_size(&not_ihdr), None);
    let truncated = png_header(1, 1);
    assert_eq!(png_size(&truncated[..32]), None);
    assert_eq!(png_size(&png_header(0, 5)), None);
    assert_eq!(png_size(&png_header(5, 0x8000_0000)), None);
    assert_eq!(png_size(b"GIF89a"), None);
}

#[test]
fn admit_png_applies_both_limits_and_digests_the_bytes() {
    let png = admit_png(png_header(CLIP_IMAGE_MAX_SIDE, 2)).expect("at the side limit");
    assert_eq!((png.width, png.height), (CLIP_IMAGE_MAX_SIDE, 2));
    assert_eq!(png.digest, <[u8; 32]>::from(Sha256::digest(&png.bytes)));
    assert_eq!(
        admit_png(png_header(CLIP_IMAGE_MAX_SIDE + 1, 2)),
        Err(PngRefusal::TooLarge {
            bytes: 33,
            width: CLIP_IMAGE_MAX_SIDE + 1,
            height: 2
        })
    );
    let mut big = png_header(10, 10);
    big.resize(CLIP_IMAGE_MAX_BYTES + 1, 0);
    assert!(matches!(admit_png(big), Err(PngRefusal::TooLarge { .. })));
    assert_eq!(admit_png(b"not a png".to_vec()), Err(PngRefusal::NotPng));
}

#[test]
fn digest_hex_round_trips() {
    let digest = <[u8; 32]>::from(Sha256::digest(b"clip"));
    let hex = digest_hex(&digest);
    assert_eq!(hex.len(), 64);
    assert_eq!(parse_digest_hex(&hex), Some(digest));
    assert_eq!(parse_digest_hex(&hex[..63]), None);
    assert_eq!(parse_digest_hex(&"g".repeat(64)), None);
}

/// The poller's view of a pasteboard with text and an image.
#[derive(Default)]
struct Board {
    count: i64,
    text: Option<String>,
    png: Option<Vec<u8>>,
    image_reads: usize,
}

impl PasteboardBackend for Board {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        self.text.clone()
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.count += 1;
        (self.text, self.png) = (Some(text.to_owned()), None);
        true
    }
    fn get_image(&mut self) -> Option<PasteboardImage> {
        self.image_reads += 1;
        self.png.clone().map(PasteboardImage::Png)
    }
    fn set_image(&mut self, png: &[u8], text: Option<&str>) -> bool {
        self.count += 1;
        (self.text, self.png) = (text.map(str::to_owned), Some(png.to_vec()));
        true
    }
}

fn image_of(clip: Option<HostClip>) -> Option<(u32, u32)> {
    match clip?.image? {
        HostImage::Png(png) => Some((png.width, png.height)),
        HostImage::TooLarge { .. } => None,
    }
}

#[test]
fn poller_without_images_never_reads_or_reports_one() {
    let mut poller = Poller::new();
    let mut board = Board { count: 1, png: Some(png_header(2, 2)), ..Board::default() };
    assert_eq!(poller.on_tick_clip(&mut board), None);
    board.count += 1;
    board.text = Some("text".into());
    assert_eq!(
        poller.on_tick_clip(&mut board),
        Some(HostClip { text: Some("text".into()), ..HostClip::default() })
    );
    assert_eq!(board.image_reads, 0);
}

#[test]
fn poller_reports_an_image_copy_once_and_never_its_own_write() {
    let mut poller = Poller::new();
    let mut board = Board { count: 1, text: Some("startup".into()), ..Board::default() };
    assert_eq!(poller.on_tick(&mut board).as_deref(), Some("startup"));
    // Switching images on reports nothing new without an image.
    assert_eq!(poller.report_images_on(&mut board), None);
    board.count += 1;
    (board.text, board.png) = (None, Some(png_header(4, 3)));
    assert_eq!(image_of(poller.on_tick_clip(&mut board)), Some((4, 3)));
    assert_eq!(poller.on_tick_clip(&mut board), None);
    // The same image copied again is a re-copy, not a new report.
    board.count += 1;
    assert_eq!(poller.on_tick_clip(&mut board), None);
    assert!(poller.observed_suppressed_change);
    // A guest image written by us does not come back.
    let guest = admit_png(png_header(9, 9)).unwrap();
    poller.apply_set_image(&mut board, &guest, Some("cells"));
    assert_eq!(board.png.as_deref(), Some(guest.bytes.as_slice()));
    assert_eq!(board.text.as_deref(), Some("cells"));
    assert_eq!(poller.on_tick_clip(&mut board), None);
    // A Mac copy since the last observation wins over a queued guest image.
    board.count += 1;
    (board.text, board.png) = (Some("mac".into()), None);
    poller.apply_set_image(&mut board, &guest, None);
    assert_eq!(board.text.as_deref(), Some("mac"));
    assert_eq!(poller.on_tick(&mut board).as_deref(), Some("mac"));
}

#[test]
fn switching_images_on_reports_a_present_image_once() {
    let mut poller = Poller::new();
    let mut board = Board { count: 1, text: Some("t".into()), png: Some(png_header(5, 5)), ..Board::default() };
    assert_eq!(poller.on_tick(&mut board).as_deref(), Some("t"));
    let clip = poller.report_images_on(&mut board);
    assert_eq!(clip.as_ref().and_then(|clip| clip.text.as_deref()), Some("t"));
    assert_eq!(image_of(clip), Some((5, 5)));
    assert_eq!(poller.on_tick_clip(&mut board), None);
}

/// A uniquely named private pasteboard, released when dropped.
struct PrivatePasteboard {
    backend: NsPasteboardBackend,
}

impl PrivatePasteboard {
    fn new(test: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let name = format!("org.bridgevm.test.{test}.{}.{nanos}", std::process::id());
        Self {
            backend: NsPasteboardBackend::named(&name),
        }
    }
    fn raw(&mut self) -> ObjcId {
        unsafe { self.backend.pasteboard() }
    }
}

impl Drop for PrivatePasteboard {
    fn drop(&mut self) {
        let pb = self.raw();
        if !pb.is_null() {
            unsafe { release_globally(pb) };
        }
    }
}

/// A 2x1 uncompressed RGB TIFF: red, then blue.
fn tiny_tiff() -> Vec<u8> {
    let entries: [(u16, u16, u32, u32); 10] = [
        (256, 3, 1, 2),     // ImageWidth
        (257, 3, 1, 1),     // ImageLength
        (258, 3, 3, 134),   // BitsPerSample -> 8,8,8 at 134
        (259, 3, 1, 1),     // Compression: none
        (262, 3, 1, 2),     // Photometric: RGB
        (273, 4, 1, 140),   // StripOffsets
        (277, 3, 1, 3),     // SamplesPerPixel
        (278, 3, 1, 1),     // RowsPerStrip
        (279, 4, 1, 6),     // StripByteCounts
        (284, 3, 1, 1),     // PlanarConfiguration: chunky
    ];
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in entries {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&kind.to_le_bytes());
        tiff.extend_from_slice(&count.to_le_bytes());
        if kind == 3 && count == 1 {
            tiff.extend_from_slice(&(value as u16).to_le_bytes());
            tiff.extend_from_slice(&[0, 0]);
        } else {
            tiff.extend_from_slice(&value.to_le_bytes());
        }
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(tiff.len(), 134);
    for _ in 0..3 {
        tiff.extend_from_slice(&8u16.to_le_bytes());
    }
    tiff.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
    tiff
}

#[test]
fn private_pasteboard_round_trips_text_and_png_in_one_write() {
    let mut pasteboard = PrivatePasteboard::new("roundtrip");
    if pasteboard.raw().is_null() || !pasteboard.backend.set_string("probe") {
        eprintln!("no pasteboard server in this session; skipped");
        return;
    }
    let png = png_header(3, 2);
    let before = pasteboard.backend.change_count();
    assert!(pasteboard.backend.set_image(&png, Some("caption")));
    assert!(pasteboard.backend.change_count() > before);
    assert_eq!(pasteboard.backend.get_string().as_deref(), Some("caption"));
    assert_eq!(
        pasteboard.backend.get_image(),
        Some(PasteboardImage::Png(png.clone()))
    );
    assert!(pasteboard.backend.set_image(&png, None));
    assert_eq!(pasteboard.backend.get_string(), None);
    assert_eq!(pasteboard.backend.get_image(), Some(PasteboardImage::Png(png)));
    assert!(pasteboard.backend.set_string("text only"));
    assert_eq!(pasteboard.backend.get_image(), None);
}

#[test]
fn private_pasteboard_tiff_becomes_png_and_file_copies_have_no_image() {
    let mut pasteboard = PrivatePasteboard::new("tiff");
    let pb = pasteboard.raw();
    if pb.is_null() || !unsafe { set_data_for_type(pb, &tiny_tiff(), c"public.tiff") } {
        eprintln!("no pasteboard server in this session; skipped");
        return;
    }
    let Some(PasteboardImage::Png(png)) = pasteboard.backend.get_image() else {
        panic!("TIFF was not converted to PNG");
    };
    assert_eq!(png_size(&png), Some((2, 1)));
    assert!(unsafe { set_data_for_type(pb, b"file:///tmp/x.png", c"public.file-url") });
    assert_eq!(pasteboard.backend.get_image(), None, "a Finder copy's icon is no image");
}
