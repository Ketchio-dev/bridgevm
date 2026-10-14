// macOS pasteboard bridge for the agent-console service loop (M6-3).
//
// A dedicated thread polls NSPasteboard generalPasteboard's changeCount and
// reports external changes over an mpsc channel; set() requests (guest->host
// clipboard sync) flow the other way. The VM tick loop stays non-blocking: it
// only ever does try_recv/send on the channels.
//
// PUBLIC API (frozen — agent_console.rs codes against exactly this):
//   HostPasteboard::spawn(poll_ms) -> HostPasteboard
//   try_changed(&self) -> Option<String>  // deduped external pasteboard changes
//   set(&self, text: String)              // guest->host sync; never echoes back
//
// Echo suppression contract: text written via set() must NOT come back out of
// try_changed(), and neither must a text equal to the last one emitted/set.
// The first poll after spawn reports the pasteboard's current content (host
// wins at startup).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
#[path = "host_pasteboard_bridge.rs"]
mod bridge;
pub use bridge::HostPasteboard;
#[cfg(test)]
pub(crate) use bridge::PasteboardWorker;

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
}

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // -1 sentinel: the first on_tick always reads the pasteboard, so
            // startup content is emitted once (host wins at startup).
            last_count: -1,
            last_seen: None,
        }
    }

    /// One poll step: Some(text) iff the pasteboard changed externally to a
    /// non-empty text we haven't already emitted or set ourselves.
    pub fn on_tick<B: PasteboardBackend>(&mut self, backend: &mut B) -> Option<String> {
        let count = backend.change_count();
        if count == self.last_count {
            return None;
        }
        self.last_count = count;
        let text = backend.get_string()?;
        if text.is_empty() || self.last_seen.as_deref() == Some(text.as_str()) {
            return None;
        }
        self.last_seen = Some(text.clone());
        Some(text)
    }

    /// Write guest text to the pasteboard, recording the post-write changeCount
    /// so the write does not re-emerge from on_tick. A changeCount that moved
    /// since the last observation is a newer Mac copy: it wins, is left in
    /// place and is reported by the next on_tick. (A copy landing between that
    /// check and set_string, or between set_string and change_count, is still
    /// missed: NSPasteboard has no compare-and-swap.)
    pub fn apply_set<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str) {
        if self.last_count != -1 && backend.change_count() != self.last_count {
            // Forget the last report so the next on_tick re-reports the Mac
            // text even if it is unchanged; the Mac copy then wins.
            self.last_seen = None;
            return;
        }
        if backend.set_string(text) {
            self.last_seen = Some(text.to_owned());
            self.last_count = backend.change_count();
        }
    }
}

fn default_backend() -> NsPasteboardBackend {
    NsPasteboardBackend::new()
}

type ObjcId = *mut c_void;
type Sel = *mut c_void;

struct NsPasteboardBackend {
    general: ObjcId,
}

// The backend is created before thread spawn and then moved into the dedicated
// pasteboard thread; after that the cached Objective-C object is only used on
// that one thread.
unsafe impl Send for NsPasteboardBackend {}

impl NsPasteboardBackend {
    fn new() -> Self {
        Self {
            general: std::ptr::null_mut(),
        }
    }

    unsafe fn pasteboard(&mut self) -> ObjcId {
        if self.general.is_null() {
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
