// macOS pasteboard bridge for the agent-console service loop (M6-3).
//
// A dedicated thread polls NSPasteboard generalPasteboard's changeCount and
// reports external changes over an mpsc channel; set() requests (guest->host
// clipboard sync) flow the other way. The VM tick loop stays non-blocking: it
// only ever does try_recv/send on the channels.
//
// PUBLIC API (agent_console.rs codes against exactly this):
//   HostPasteboard::spawn(poll_ms) -> HostPasteboard
//   try_changed_clip(&self) -> Option<HostClip>  // deduped external changes
//   set(&self, text: String)                     // guest->host text sync
//   set_image(&self, png, text: Option<String>)  // guest->host image (+text)
//   set_images(&self, on: bool)                  // read images too (IMAGE_PNG)
//
// Echo suppression contract: content written via set()/set_image() must NOT
// come back out of try_changed_clip(), and neither must content equal to the
// last one emitted/set. A refused queued write may re-report that content to
// restore host ownership. The first poll after spawn reports the pasteboard's
// current content (host wins at startup); so does the first poll after images
// are switched on, when the pasteboard holds an image.
// BRIDGEVM_TEST_CLIP_PASTEBOARD=<name> swaps the general pasteboard for a
// private named one, so live checks never touch the user's clipboard.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
#[path = "host_pasteboard_bridge.rs"]
mod bridge;
pub use bridge::{HostClip, HostPasteboard};
#[cfg(test)]
pub(crate) use bridge::PasteboardWorker;
#[path = "host_pasteboard_image.rs"]
mod image;
pub use image::*;

/// Test-only: the name of a private pasteboard used instead of the user's.
pub const TEST_PASTEBOARD_ENV: &str = "BRIDGEVM_TEST_CLIP_PASTEBOARD";

/// Minimal pasteboard surface, abstracted so the poller logic is unit-testable
/// without touching the real NSPasteboard (tests must never clobber the user's
/// clipboard).
pub trait PasteboardBackend: Send {
    /// NSPasteboard changeCount — bumps on every ownership change.
    fn change_count(&mut self) -> i64;
    /// Current plain-text content (public.utf8-plain-text), if any.
    fn get_string(&mut self) -> Option<String>;
    /// Replace the pasteboard content with `text`. Returns false on failure.
    fn set_string(&mut self, text: &str) -> bool;
    /// The pasteboard's image as PNG (only read while images are on).
    fn get_image(&mut self) -> Option<PasteboardImage> {
        None
    }
    /// Replace the content with `text` (first, when given) and `png`.
    fn set_image(&mut self, png: &[u8], text: Option<&str>) -> bool {
        let _ = (png, text);
        false
    }
}

#[path = "host_pasteboard_poller.rs"]
mod poller;
pub use poller::Poller;

fn default_backend() -> NsPasteboardBackend {
    let name = std::env::var(TEST_PASTEBOARD_ENV)
        .ok()
        .filter(|name| !name.is_empty())
        .and_then(|name| CString::new(name).ok());
    NsPasteboardBackend { general: std::ptr::null_mut(), name }
}

type ObjcId = *mut c_void;
type Sel = *mut c_void;

pub(crate) struct NsPasteboardBackend {
    general: ObjcId,
    /// TEST_PASTEBOARD_ENV's private pasteboard instead of the general one.
    name: Option<CString>,
}

// The backend is created before thread spawn and then moved into the dedicated
// pasteboard thread; after that the cached Objective-C object is only used on
// that one thread.
unsafe impl Send for NsPasteboardBackend {}

impl NsPasteboardBackend {
    #[cfg(test)]
    pub(crate) fn named(name: &str) -> Self {
        Self {
            general: std::ptr::null_mut(),
            name: Some(CString::new(name).expect("pasteboard name")),
        }
    }

    unsafe fn pasteboard(&mut self) -> ObjcId {
        if self.general.is_null() {
            if let Some(name) = self.name.as_ref() {
                self.general = image::named_pasteboard(name);
                return self.general;
            }
            let class = objc_getClass(c"NSPasteboard".as_ptr().cast());
            if class.is_null() {
                return std::ptr::null_mut();
            }
            // generalPasteboard is a per-process singleton; caching avoids a
            // class-message round trip on every poll without adding ownership
            // work for the one-thread backend.
            self.general = msg_send_id(class, sel(b"generalPasteboard\0"));
        }
        self.general
    }
}

impl PasteboardBackend for NsPasteboardBackend {
    fn change_count(&mut self) -> i64 {
        with_autorelease_pool(|| unsafe {
            let pb = self.pasteboard();
            if pb.is_null() {
                return 0;
            }
            msg_send_i64(pb, sel(b"changeCount\0"))
        })
    }

    fn get_string(&mut self) -> Option<String> {
        with_autorelease_pool(|| unsafe {
            let pb = self.pasteboard();
            if pb.is_null() {
                return None;
            }
            let ty = plain_text_type_string();
            if ty.is_null() {
                return None;
            }
            let ns_text = msg_send_id_id(pb, sel(b"stringForType:\0"), ty);
            if ns_text.is_null() {
                return None;
            }
            let bytes = msg_send_cstr(ns_text, sel(b"UTF8String\0"));
            if bytes.is_null() {
                return None;
            }
            // UTF8String storage is tied to the autoreleased NSString, so copy
            // it into Rust before popping the pool at the end of this method.
            Some(CStr::from_ptr(bytes).to_string_lossy().into_owned())
        })
    }

    fn set_string(&mut self, text: &str) -> bool {
        with_autorelease_pool(|| unsafe {
            let pb = self.pasteboard();
            if pb.is_null() {
                return false;
            }
            let ty = plain_text_type_string();
            if ty.is_null() {
                return false;
            }
            let sanitized: Vec<u8> = text
                .as_bytes()
                .iter()
                .copied()
                .filter(|byte| *byte != 0)
                .collect();
            let c_text = match CString::new(sanitized) {
                Ok(c_text) => c_text,
                Err(_) => return false,
            };
            let ns_text = ns_string_from_utf8(c_text.as_ptr());
            if ns_text.is_null() {
                return false;
            }
            let _ = msg_send_i64(pb, sel(b"clearContents\0"));
            msg_send_bool_id_id(pb, sel(b"setString:forType:\0"), ns_text, ty)
        })
    }

    fn get_image(&mut self) -> Option<PasteboardImage> {
        with_autorelease_pool(|| unsafe {
            let pb = self.pasteboard();
            if pb.is_null() {
                return None;
            }
            image::read_image(pb)
        })
    }

    fn set_image(&mut self, png: &[u8], text: Option<&str>) -> bool {
        with_autorelease_pool(|| unsafe {
            let pb = self.pasteboard();
            !pb.is_null() && image::write_image(pb, png, text)
        })
    }
}

fn with_autorelease_pool<R>(body: impl FnOnce() -> R) -> R {
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let result = body();
        objc_autoreleasePoolPop(pool);
        result
    }
}

unsafe fn plain_text_type_string() -> ObjcId {
    // Build the pasteboard type NSString instead of linking the
    // NSPasteboardTypeString constant symbol; this keeps the FFI surface to
    // Objective-C runtime calls and avoids an extra AppKit extern symbol.
    ns_string_from_utf8(c"public.utf8-plain-text".as_ptr().cast())
}

unsafe fn ns_string_from_utf8(bytes: *const c_char) -> ObjcId {
    let class = objc_getClass(c"NSString".as_ptr().cast());
    if class.is_null() {
        return std::ptr::null_mut();
    }
    msg_send_id_cstr(class, sel(b"stringWithUTF8String:\0"), bytes)
}

unsafe fn sel(name: &'static [u8]) -> Sel {
    debug_assert_eq!(name.last(), Some(&0));
    sel_registerName(name.as_ptr().cast())
}

unsafe fn msg_send_id(receiver: ObjcId, selector: Sel) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> ObjcId;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

unsafe fn msg_send_i64(receiver: ObjcId, selector: Sel) -> i64 {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> i64;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

unsafe fn msg_send_cstr(receiver: ObjcId, selector: Sel) -> *const c_char {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> *const c_char;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector)
}

unsafe fn msg_send_id_cstr(receiver: ObjcId, selector: Sel, arg: *const c_char) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, *const c_char) -> ObjcId;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

unsafe fn msg_send_id_id(receiver: ObjcId, selector: Sel, arg: ObjcId) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, ObjcId) -> ObjcId;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg)
}

unsafe fn msg_send_bool_id_id(receiver: ObjcId, selector: Sel, arg1: ObjcId, arg2: ObjcId) -> bool {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, ObjcId, ObjcId) -> c_char;
    let send: MsgSend = std::mem::transmute(objc_msgSend as *const ());
    send(receiver, selector, arg1, arg2) != 0
}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> ObjcId;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> ObjcId;
    fn objc_autoreleasePoolPop(pool: ObjcId);
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[cfg(test)]
#[path = "host_pasteboard_tests.rs"]
mod tests;
