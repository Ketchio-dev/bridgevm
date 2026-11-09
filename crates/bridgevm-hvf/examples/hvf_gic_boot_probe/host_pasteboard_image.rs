//! Clipboard images: the limits, the one check a PNG gets before it crosses
//! the trust boundary (signature and IHDR size; nothing past the header is
//! decoded), and NSPasteboard reads and writes. The Mac side reads public.png,
//! or public.tiff converted to PNG here, and writes public.png.

use super::{
    msg_send_bool_id_id, msg_send_i64, msg_send_id, msg_send_id_id, ns_string_from_utf8,
    objc_getClass, sel, ObjcId, Sel,
};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::os::raw::{c_char, c_void};

/// Largest clipboard PNG, in bytes, moved in either direction.
pub const CLIP_IMAGE_MAX_BYTES: usize = 32 * 1024 * 1024;
/// Largest clipboard image side, in pixels, moved in either direction.
pub const CLIP_IMAGE_MAX_SIDE: u32 = 16384;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// Signature plus the whole IHDR chunk (length, type, 13 data bytes, CRC).
const PNG_IHDR_END: usize = 33;
/// NSBitmapImageFileTypePNG.
const NS_BITMAP_IMAGE_FILE_TYPE_PNG: u64 = 4;

/// A clipboard image within both limits, as PNG bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// SHA-256 of `bytes`: the image's identity for echo suppression.
    pub digest: [u8; 32],
}

/// The image a Mac pasteboard change carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostImage {
    Png(PngImage),
    /// Stays on the Mac: over a limit. `bytes` is 0 when no PNG was made.
    TooLarge {
        bytes: usize,
        width: u32,
        height: u32,
    },
}

/// What a backend found on its pasteboard, before the PNG check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteboardImage {
    Png(Vec<u8>),
    TooLarge {
        bytes: usize,
        width: u32,
        height: u32,
    },
}

/// Why a PNG does not cross.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngRefusal {
    NotPng,
    TooLarge {
        bytes: usize,
        width: u32,
        height: u32,
    },
}

/// Width and height from a PNG's IHDR, or None when `bytes` does not start
/// with a PNG signature and a complete IHDR chunk.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let header = bytes.get(..PNG_IHDR_END)?;
    if header[..8] != PNG_SIGNATURE || header[8..12] != [0, 0, 0, 13] || &header[12..16] != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    let side = 1..=i32::MAX as u32;
    (side.contains(&width) && side.contains(&height)).then_some((width, height))
}

pub fn within_limits(bytes: usize, width: u32, height: u32) -> bool {
    bytes <= CLIP_IMAGE_MAX_BYTES && width <= CLIP_IMAGE_MAX_SIDE && height <= CLIP_IMAGE_MAX_SIDE
}

/// The PNG that may cross: a PNG header within both limits, with its digest.
pub fn admit_png(bytes: Vec<u8>) -> Result<PngImage, PngRefusal> {
    let (width, height) = png_size(&bytes).ok_or(PngRefusal::NotPng)?;
    if !within_limits(bytes.len(), width, height) {
        return Err(PngRefusal::TooLarge {
            bytes: bytes.len(),
            width,
            height,
        });
    }
    let digest = Sha256::digest(&bytes).into();
    Ok(PngImage {
        bytes,
        width,
        height,
        digest,
    })
}

/// The host-side reading of a backend's image: a checked PNG or a refusal.
/// A pasteboard PNG that is not a PNG is no image at all.
pub fn host_image(image: PasteboardImage) -> Option<HostImage> {
    match image {
        PasteboardImage::Png(bytes) => match admit_png(bytes) {
            Ok(png) => Some(HostImage::Png(png)),
            Err(PngRefusal::TooLarge {
                bytes,
                width,
                height,
            }) => Some(HostImage::TooLarge {
                bytes,
                width,
                height,
            }),
            Err(PngRefusal::NotPng) => None,
        },
        PasteboardImage::TooLarge {
            bytes,
            width,
            height,
        } => Some(HostImage::TooLarge {
            bytes,
            width,
            height,
        }),
    }
}

#[cfg(test)]
pub fn digest_hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    digest.iter().fold(String::with_capacity(64), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

pub fn parse_digest_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(digest)
}

/// The pasteboard's image: public.png, else public.tiff converted to PNG.
/// A Finder file copy (public.file-url) carries the file's icon as an image,
/// so it has none. Sizes are checked before anything large is copied.
pub(super) unsafe fn read_image(pb: ObjcId) -> Option<PasteboardImage> {
    if has_type(pb, c"public.file-url") {
        return None;
    }
    if let Some(data) = data_for_type(pb, c"public.png") {
        let len = data_len(data);
        if len > CLIP_IMAGE_MAX_BYTES {
            let head = data_bytes(data, PNG_IHDR_END.min(len));
            let (width, height) = png_size(&head).unwrap_or((0, 0));
            return Some(PasteboardImage::TooLarge {
                bytes: len,
                width,
                height,
            });
        }
        let bytes = data_bytes(data, len);
        if png_size(&bytes).is_some() {
            return Some(PasteboardImage::Png(bytes));
        }
    }
    let tiff = data_for_type(pb, c"public.tiff")?;
    let rep = msg_send_id_id(
        class(c"NSBitmapImageRep"),
        sel(b"imageRepWithData:\0"),
        tiff,
    );
    if rep.is_null() {
        return None;
    }
    let side = |pixels: i64| u32::try_from(pixels).unwrap_or(0);
    let width = side(msg_send_i64(rep, sel(b"pixelsWide\0")));
    let height = side(msg_send_i64(rep, sel(b"pixelsHigh\0")));
    if !within_limits(0, width, height) {
        return Some(PasteboardImage::TooLarge {
            bytes: 0,
            width,
            height,
        });
    }
    let properties = msg_send_id(class(c"NSDictionary"), sel(b"dictionary\0"));
    let png = msg_send_id_u64_id(
        rep,
        sel(b"representationUsingType:properties:\0"),
        NS_BITMAP_IMAGE_FILE_TYPE_PNG,
        properties,
    );
    if png.is_null() {
        return None;
    }
    let len = data_len(png);
    if len > CLIP_IMAGE_MAX_BYTES {
        return Some(PasteboardImage::TooLarge {
            bytes: len,
            width,
            height,
        });
    }
    Some(PasteboardImage::Png(data_bytes(png, len)))
}

/// Replace the pasteboard content with `text` (first, when given) and `png`
/// as public.png, in one clearContents.
pub(super) unsafe fn write_image(pb: ObjcId, png: &[u8], text: Option<&str>) -> bool {
    let data = msg_send_id_ptr_usize(
        class(c"NSData"),
        sel(b"dataWithBytes:length:\0"),
        png.as_ptr().cast(),
        png.len(),
    );
    let png_type = ns_string_from_utf8(c"public.png".as_ptr());
    let text = match text {
        Some(text) => {
            let sanitized: Vec<u8> = text.bytes().filter(|byte| *byte != 0).collect();
            let Ok(c_text) = CString::new(sanitized) else {
                return false;
            };
            let ns_text = ns_string_from_utf8(c_text.as_ptr());
            let text_type = ns_string_from_utf8(c"public.utf8-plain-text".as_ptr());
            if ns_text.is_null() || text_type.is_null() {
                return false;
            }
            Some((ns_text, text_type))
        }
        None => None,
    };
    if data.is_null() || png_type.is_null() {
        return false;
    }
    let _ = msg_send_i64(pb, sel(b"clearContents\0"));
    if let Some((ns_text, text_type)) = text {
        if !msg_send_bool_id_id(pb, sel(b"setString:forType:\0"), ns_text, text_type) {
            return false;
        }
    }
    msg_send_bool_id_id(pb, sel(b"setData:forType:\0"), data, png_type)
}

/// `[NSPasteboard pasteboardWithName:]`, retained for the process: the
/// BRIDGEVM_TEST_CLIP_PASTEBOARD private pasteboard of live checks.
pub(super) unsafe fn named_pasteboard(name: &CString) -> ObjcId {
    let ns_name = ns_string_from_utf8(name.as_ptr());
    if ns_name.is_null() {
        return std::ptr::null_mut();
    }
    let pb = msg_send_id_id(
        class(c"NSPasteboard"),
        sel(b"pasteboardWithName:\0"),
        ns_name,
    );
    if pb.is_null() {
        return pb;
    }
    msg_send_id(pb, sel(b"retain\0"))
}

pub(super) unsafe fn class(name: &std::ffi::CStr) -> ObjcId {
    objc_getClass(name.as_ptr())
}

pub(super) unsafe fn has_type(pb: ObjcId, ty: &std::ffi::CStr) -> bool {
    let types = msg_send_id(pb, sel(b"types\0"));
    let ty = ns_string_from_utf8(ty.as_ptr());
    !types.is_null() && !ty.is_null() && msg_send_bool_id(types, sel(b"containsObject:\0"), ty)
}

pub(super) unsafe fn data_for_type(pb: ObjcId, ty: &std::ffi::CStr) -> Option<ObjcId> {
    let ty = ns_string_from_utf8(ty.as_ptr());
    if ty.is_null() {
        return None;
    }
    let data = msg_send_id_id(pb, sel(b"dataForType:\0"), ty);
    (!data.is_null()).then_some(data)
}

pub(super) unsafe fn data_len(data: ObjcId) -> usize {
    msg_send_usize(data, sel(b"length\0"))
}

/// The first `len` bytes of an NSData, copied out of the autoreleased object.
pub(super) unsafe fn data_bytes(data: ObjcId, len: usize) -> Vec<u8> {
    let ptr = msg_send_ptr(data, sel(b"bytes\0")).cast::<u8>();
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(ptr, len.min(data_len(data))).to_vec()
}

unsafe fn msg_send_usize(receiver: ObjcId, selector: Sel) -> usize {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> usize;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector)
}

unsafe fn msg_send_ptr(receiver: ObjcId, selector: Sel) -> *const c_void {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> *const c_void;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector)
}

pub(super) unsafe fn msg_send_bool_id(receiver: ObjcId, selector: Sel, arg: ObjcId) -> bool {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, ObjcId) -> c_char;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, arg) != 0
}

unsafe fn msg_send_id_u64_id(receiver: ObjcId, selector: Sel, arg1: u64, arg2: ObjcId) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, u64, ObjcId) -> ObjcId;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, arg1, arg2)
}

pub(super) unsafe fn msg_send_id_ptr_usize(
    receiver: ObjcId,
    selector: Sel,
    arg1: *const c_void,
    arg2: usize,
) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, *const c_void, usize) -> ObjcId;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, arg1, arg2)
}

#[cfg(test)]
pub(super) unsafe fn set_data_for_type(pb: ObjcId, bytes: &[u8], ty: &std::ffi::CStr) -> bool {
    let data = msg_send_id_ptr_usize(
        class(c"NSData"),
        sel(b"dataWithBytes:length:\0"),
        bytes.as_ptr().cast(),
        bytes.len(),
    );
    let ty = ns_string_from_utf8(ty.as_ptr());
    let _ = msg_send_i64(pb, sel(b"clearContents\0"));
    msg_send_bool_id_id(pb, sel(b"setData:forType:\0"), data, ty)
}

#[cfg(test)]
pub(super) unsafe fn release_globally(pb: ObjcId) {
    let _ = msg_send_id(pb, sel(b"releaseGlobally\0"));
}

#[cfg(test)]
#[path = "host_pasteboard_image_tests.rs"]
mod tests;
