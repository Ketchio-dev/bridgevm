//! Mac apps that copy rich text as RTF alone (TextEdit, and any NSTextView)
//! write no public.html. Their RTF is read as HTML here, through
//! NSAttributedString's own HTML writer, so it reaches Windows formatted.

use super::image::{class, data_bytes, data_for_type, data_len};
use super::{msg_send_id, objc_msgSend, sel, ObjcId, Sel};

#[repr(C)]
struct NsRange {
    location: usize,
    length: usize,
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSDocumentTypeDocumentAttribute: ObjcId;
    static NSHTMLTextDocumentType: ObjcId;
}

/// The pasteboard's public.rtf as an HTML document, or None without RTF.
pub(super) unsafe fn rtf_as_html(pb: ObjcId) -> Option<String> {
    let rtf = data_for_type(pb, c"public.rtf")?;
    type InitWithRtf = unsafe extern "C" fn(ObjcId, Sel, ObjcId, *mut ObjcId) -> ObjcId;
    let init: InitWithRtf = std::mem::transmute(objc_msgSend as *const ());
    let alloc = msg_send_id(class(c"NSAttributedString"), sel(b"alloc\0"));
    let text = init(alloc, sel(b"initWithRTF:documentAttributes:\0"), rtf, std::ptr::null_mut());
    if text.is_null() {
        return None;
    }
    type DictionaryWith = unsafe extern "C" fn(ObjcId, Sel, ObjcId, ObjcId) -> ObjcId;
    let dictionary: DictionaryWith = std::mem::transmute(objc_msgSend as *const ());
    let attributes = dictionary(
        class(c"NSDictionary"),
        sel(b"dictionaryWithObject:forKey:\0"),
        NSHTMLTextDocumentType,
        NSDocumentTypeDocumentAttribute,
    );
    type Length = unsafe extern "C" fn(ObjcId, Sel) -> usize;
    let length: Length = std::mem::transmute(objc_msgSend as *const ());
    let range = NsRange {
        location: 0,
        length: length(text, sel(b"length\0")),
    };
    type DataFrom = unsafe extern "C" fn(ObjcId, Sel, NsRange, ObjcId, *mut ObjcId) -> ObjcId;
    let data_from: DataFrom = std::mem::transmute(objc_msgSend as *const ());
    let html = data_from(
        text,
        sel(b"dataFromRange:documentAttributes:error:\0"),
        range,
        attributes,
        std::ptr::null_mut(),
    );
    let _ = msg_send_id(text, sel(b"release\0"));
    if html.is_null() {
        return None;
    }
    // Cocoa's HTML writer emits UTF-8 and says so in the document's head.
    String::from_utf8(data_bytes(html, data_len(html))).ok()
}
