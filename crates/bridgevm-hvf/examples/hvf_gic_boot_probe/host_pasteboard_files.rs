//! Mac file copies (a Finder copy's file URLs) on their way to Windows: the
//! pasteboard read and the walk below the copied items, on the pasteboard
//! thread. The walk never follows a symlink and skips anything that is not a
//! regular file or a folder, so no name or byte outside the copied items is
//! read. Names go as Windows stores them (share_guest_names.rs: composed, with
//! stand-ins for the characters Windows refuses); one with a control character
//! (Finder's custom-icon file is `Icon\r`) is left out.

use super::image::{class, has_type};
use super::{msg_send_cstr, msg_send_id, msg_send_id_id, ns_string_from_utf8, sel, ObjcId, Sel};
use crate::agent_console::clip_files::{CLIP_FILES_MAX_BYTES, CLIP_FILES_MAX_ENTRIES};
use crate::agent_console::share_sync::{composed, guest_char};
use sha2::{Digest, Sha256};
use std::ffi::CStr;
use std::io::Read as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "host_pasteboard_files_tests.rs"]
mod tests;

/// The files of a Mac copy, as they go to Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostFiles {
    Read(HostFileCopy),
    /// Over a limit: what the walk counted when it stopped.
    TooLarge { entries: usize, bytes: u64 },
}

/// A copy within the limits, read whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFileCopy {
    /// Each folder before what is in it, by its Windows path in the copy
    /// (`\` between names); a file with its bytes.
    pub entries: Vec<HostFileEntry>,
    pub bytes: u64,
    /// SHA-256 of the paths and bytes: the copy's identity for echo suppression.
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFileEntry {
    pub rel: String,
    /// None for a folder.
    pub data: Option<Vec<u8>>,
}

/// A path in composed form (NFC): AppKit hands out file URL paths decomposed,
/// and APFS opens either form, so paths compare in this one.
pub fn composed_path(path: &Path) -> PathBuf {
    path.to_str().map_or_else(|| path.to_path_buf(), |path| PathBuf::from(composed(path).into_owned()))
}

/// A Mac name as Windows can store and show it; None when Windows cannot.
fn windows_name(name: &str) -> Option<String> {
    (!name.chars().any(char::is_control)).then(|| composed(name).chars().map(guest_char).collect())
}

/// The copied items and everything below them, depth first with each folder's
/// names sorted; None when no item is a regular file or a folder.
pub fn host_files(tops: &[PathBuf]) -> Option<HostFiles> {
    let mut found: Vec<(String, PathBuf, bool)> = Vec::new();
    let mut bytes = 0u64;
    let mut pending: Vec<(PathBuf, String)> = tops
        .iter()
        .rev()
        .filter_map(|top| Some((top.clone(), windows_name(top.file_name()?.to_str()?)?)))
        .collect();
    while let Some((path, rel)) = pending.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let mut names: Vec<(String, String)> = std::fs::read_dir(&path)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter_map(|name| Some((windows_name(&name)?, name)))
                .collect();
            names.sort();
            pending.extend(names.iter().rev().map(|(win, name)| (path.join(name), format!("{rel}\\{win}"))));
        } else if meta.is_file() {
            bytes = bytes.saturating_add(meta.len());
        } else {
            continue;
        }
        found.push((rel, path, meta.is_dir()));
        if found.len() > CLIP_FILES_MAX_ENTRIES || bytes > CLIP_FILES_MAX_BYTES {
            return Some(HostFiles::TooLarge { entries: found.len(), bytes });
        }
    }
    if found.is_empty() {
        return None;
    }
    // Read only now, each file at most to the limit: one may have grown since.
    let (mut entries, mut bytes, mut digest) = (Vec::with_capacity(found.len()), 0u64, Sha256::new());
    for (rel, path, dir) in found {
        let data = if dir {
            None
        } else {
            let Some(data) = read_regular(&path, CLIP_FILES_MAX_BYTES - bytes) else {
                continue;
            };
            bytes += data.len() as u64;
            if bytes > CLIP_FILES_MAX_BYTES {
                return Some(HostFiles::TooLarge { entries: entries.len() + 1, bytes });
            }
            Some(data)
        };
        digest.update(rel.as_bytes());
        digest.update([0]);
        if let Some(data) = data.as_deref() {
            digest.update((data.len() as u64).to_le_bytes());
            digest.update(data);
        }
        entries.push(HostFileEntry { rel, data });
    }
    Some(HostFiles::Read(HostFileCopy {
        entries,
        bytes,
        digest: digest.finalize().into(),
    }))
}

/// A regular file's bytes, at most `limit + 1` of them; None when it cannot
/// be read or is a link now.
fn read_regular(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut data = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut data).ok()?;
    Some(data)
}

/// The pasteboard's file URLs as composed paths, in pasteboard order (a Finder
/// copy holds file reference URLs). Each URL read makes AppKit log one
/// `sandbox_extension_consume` line in an unsandboxed process.
pub(super) unsafe fn read_file_urls(pb: ObjcId) -> Vec<PathBuf> {
    if !has_type(pb, c"public.file-url") {
        return Vec::new();
    }
    let classes = msg_send_id_id(class(c"NSArray"), sel(b"arrayWithObject:\0"), class(c"NSURL"));
    let yes = msg_send_id_bool(class(c"NSNumber"), sel(b"numberWithBool:\0"), true);
    let key = ns_string_from_utf8(c"NSPasteboardURLReadingFileURLsOnlyKey".as_ptr());
    let options = msg_send_id_id_id(class(c"NSDictionary"), sel(b"dictionaryWithObject:forKey:\0"), yes, key);
    let urls = msg_send_id_id_id(pb, sel(b"readObjectsForClasses:options:\0"), classes, options);
    if urls.is_null() {
        return Vec::new();
    }
    (0..msg_send_usize(urls, sel(b"count\0")))
        .filter_map(|index| {
            let url = msg_send_id(msg_send_id_usize(urls, sel(b"objectAtIndex:\0"), index), sel(b"filePathURL\0"));
            let path = (!url.is_null()).then(|| msg_send_id(url, sel(b"path\0")))?;
            let utf8 = (!path.is_null()).then(|| msg_send_cstr(path, sel(b"UTF8String\0")))?;
            let path = (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())?;
            Some(composed_path(Path::new(&path)))
        })
        .collect()
}

unsafe fn msg_send_usize(receiver: ObjcId, selector: Sel) -> usize {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel) -> usize;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector)
}

unsafe fn msg_send_id_usize(receiver: ObjcId, selector: Sel, arg: usize) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, usize) -> ObjcId;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, arg)
}

unsafe fn msg_send_id_bool(receiver: ObjcId, selector: Sel, arg: bool) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, i8) -> ObjcId;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, i8::from(arg))
}

unsafe fn msg_send_id_id_id(receiver: ObjcId, selector: Sel, arg1: ObjcId, arg2: ObjcId) -> ObjcId {
    type MsgSend = unsafe extern "C" fn(ObjcId, Sel, ObjcId, ObjcId) -> ObjcId;
    let send: MsgSend = std::mem::transmute(super::objc_msgSend as *const ());
    send(receiver, selector, arg1, arg2)
}
