//! Mac names as Windows can store and show them.
//!
//! - Foundation writes file names decomposed (NFD). Windows keeps the code
//!   points it is given and Explorer then shows Korean as loose jamo, so share
//!   keys are composed (NFC). APFS finds a file under either form.
//! - Windows refuses `" * : < > ? |`, all legal in a Mac name (Finder shows `:`
//!   as `/`). The guest name carries them as the private-use characters the
//!   macOS SMB client uses (U+F020...), and a guest name maps back. A Mac
//!   name that already holds one of those characters stays out of the share:
//!   it would come back renamed.

use std::borrow::Cow;

/// Mac character and the guest character that stands for it.
const FORBIDDEN: [(char, char); 7] = [
    ('"', '\u{F020}'),
    ('*', '\u{F021}'),
    (':', '\u{F022}'),
    ('<', '\u{F023}'),
    ('>', '\u{F024}'),
    ('?', '\u{F025}'),
    ('|', '\u{F027}'),
];

/// The guest character for one character of a share key.
pub fn guest_char(c: char) -> char {
    FORBIDDEN
        .iter()
        .find(|(mac, _)| *mac == c)
        .map_or(c, |(_, guest)| *guest)
}

/// A character that stands in for one Windows refuses.
pub fn is_stand_in(c: char) -> bool {
    FORBIDDEN.iter().any(|(_, guest)| *guest == c)
}

fn mac_char(c: char) -> char {
    FORBIDDEN
        .iter()
        .find(|(_, guest)| *guest == c)
        .map_or(c, |(mac, _)| *mac)
}

/// Combining marks that NFC can fold into the letter before them: accents,
/// Hangul jamo and kana sound marks. Any other name is kept as it is, which
/// keeps `composed` idempotent and nearly free for most names. Both sides
/// apply it per path component, so a name it leaves alone stays alone.
fn may_compose(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{0653}'..='\u{0655}' | '\u{1100}'..='\u{11FF}' | '\u{3099}'..='\u{309A}')
}

/// The composed (NFC) form of a name.
pub fn composed(name: &str) -> Cow<'_, str> {
    if !name.chars().any(may_compose) {
        return Cow::Borrowed(name);
    }
    nfc(name).map_or(Cow::Borrowed(name), Cow::Owned)
}

/// The share key for a listed guest name (`/` separated). Each component is
/// composed on its own, as the host scan composes it, so one component's
/// marks never decide another's form.
pub fn key_from_guest(name: &str) -> Cow<'_, str> {
    if !name.chars().any(|c| may_compose(c) || mac_char(c) != c) {
        return Cow::Borrowed(name);
    }
    let mut key = String::with_capacity(name.len());
    for (i, part) in name.split('/').enumerate() {
        if i > 0 {
            key.push('/');
        }
        let mac: String = part.chars().map(mac_char).collect();
        key.push_str(&composed(&mac));
    }
    Cow::Owned(key)
}

fn nfc(name: &str) -> Option<String> {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    let len = isize::try_from(name.len()).ok()?;
    // SAFETY: each CF object is checked for null and released once; the byte
    // buffer is sized by the first CFStringGetBytes call.
    unsafe {
        let source = CFStringCreateWithBytes(null(), name.as_ptr(), len, UTF8, 0);
        if source.is_null() {
            return None;
        }
        let text = CFStringCreateMutableCopy(null(), 0, source);
        CFRelease(source);
        if text.is_null() {
            return None;
        }
        CFStringNormalize(text, FORM_C);
        let range = CFRange {
            location: 0,
            length: CFStringGetLength(text),
        };
        let mut used = 0;
        CFStringGetBytes(text, range, UTF8, 0, 0, null_mut(), 0, &mut used);
        let mut out = vec![0u8; usize::try_from(used).unwrap_or(0)];
        CFStringGetBytes(text, range, UTF8, 0, 0, out.as_mut_ptr(), used, &mut used);
        CFRelease(text as *const c_void);
        String::from_utf8(out).ok()
    }
}

const UTF8: u32 = 0x0800_0100;
const FORM_C: isize = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithBytes(
        alloc: *const std::ffi::c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external: u8,
    ) -> *const std::ffi::c_void;
    fn CFStringCreateMutableCopy(
        alloc: *const std::ffi::c_void,
        max_length: isize,
        string: *const std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn CFStringNormalize(string: *mut std::ffi::c_void, form: isize);
    fn CFStringGetLength(string: *const std::ffi::c_void) -> isize;
    fn CFStringGetBytes(
        string: *const std::ffi::c_void,
        range: CFRange,
        encoding: u32,
        loss_byte: u8,
        is_external: u8,
        buffer: *mut u8,
        max_len: isize,
        used: *mut isize,
    ) -> isize;
    fn CFRelease(object: *const std::ffi::c_void);
}

#[cfg(test)]
#[path = "share_guest_names_tests.rs"]
mod tests;
