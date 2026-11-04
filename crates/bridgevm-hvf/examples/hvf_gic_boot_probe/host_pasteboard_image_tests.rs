//! The PNG header check, and NSPasteboard image reads and writes on a private
//! uniquely named pasteboard (never the user's clipboard).

use super::super::{NsPasteboardBackend, PasteboardBackend};
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
