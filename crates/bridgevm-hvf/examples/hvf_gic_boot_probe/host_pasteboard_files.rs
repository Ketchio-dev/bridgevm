//! Mac file copies (a Finder copy's file URLs) on their way to Windows: the
//! pasteboard read and the walk below the copied items, on the pasteboard
//! thread. The walk never follows a symlink and skips anything that is not a
//! regular file or a folder, so no name or byte outside the copied items is
//! read. Names go as Windows stores them (share_guest_names.rs: composed, with
//! stand-ins for the characters Windows refuses); one with a control character
//! (Finder's custom-icon file is `Icon\r`) is left out.
//!
//! No file is read here: the walk opens each one (so an unreadable file fails
//! the copy now) and keeps its size and times, and the push reads it a chunk at
//! a time (clip_files_push.rs), so a copy of many GB never sits in memory.

use super::image::{class, has_type};
use super::{msg_send_cstr, msg_send_id, msg_send_id_id, ns_string_from_utf8, sel, ObjcId, Sel};
use crate::agent_console::clip_files::{CLIP_FILES_BIG_MAX_BYTES, CLIP_FILES_BIG_MAX_ENTRIES};
use crate::agent_console::share::{source_version, SourceVersion};
use crate::agent_console::share_sync::{composed, guest_char};
use sha2::{Digest, Sha256};
use std::ffi::CStr;
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
    /// Something below the copied items could not be listed or read (its
    /// permissions, say): sent whole or not at all. What the walk counted.
    Unreadable { entries: usize, bytes: u64 },
}

/// A copy within the walk's limits (the largest any agent takes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFileCopy {
    /// Each folder before what is in it, by its Windows path in the copy
    /// (`\` between names); a file with where it is read from.
    pub entries: Vec<HostFileEntry>,
    pub bytes: u64,
    /// SHA-256 of the paths and each file's size and times: the copy's identity
    /// for echo suppression.
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFileEntry {
    pub rel: String,
    /// None for a folder.
    pub file: Option<HostFileSource>,
}

/// A regular file of the copy as the walk found it; sent only while it stays so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFileSource {
    pub path: PathBuf,
    /// Its size and modification and change times when the walk opened it.
    pub version: SourceVersion,
}

impl HostFileSource {
    pub fn bytes(&self) -> u64 {
        self.version.0
    }
}

/// A path in composed form (NFC): AppKit hands out file URL paths decomposed,
/// and APFS opens either form, so paths compare in this one.
pub fn composed_path(path: &Path) -> PathBuf {
    path.to_str().map_or_else(|| path.to_path_buf(), |path| PathBuf::from(composed(path).into_owned()))
}

/// A Mac name as Windows can store and show it; None when Windows cannot. A
/// `\` (legal in a Mac name) would be a separator in the guest path, so such
/// a name is left out like a control character. Win32 drops a name's trailing
/// dots and spaces (two names would meet in one file), so they go here, and a
/// device name (`nul.txt`) is no file there.
fn windows_name(name: &str) -> Option<String> {
    if name.chars().any(|c| c.is_control() || c == '\\') {
        return None;
    }
    let name: String = composed(name).chars().map(guest_char).collect();
    let name = name.trim_end_matches(['.', ' ']);
    let stem = name.split('.').next().unwrap_or_default().trim_end_matches(' ').to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || matches!(stem.as_bytes(), [b'C', b'O', b'M', b'1'..=b'9'] | [b'L', b'P', b'T', b'1'..=b'9']);
    (!name.is_empty() && !device).then(|| name.to_owned())
}

/// A name as NTFS compares it: each character by its simple uppercase, as the
/// volume's upcase table maps it (`σ` and `ς` are both `Σ`; `ß` stays `ß`).
fn ntfs_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(upper), None) if (c as u32) < 0x1_0000 => upper,
                _ => c,
            }
        })
        .collect()
}

/// `name`, or as Explorer numbers a second item of the name, `name (2)`
/// before its extension: Windows names ignore case. `taken` holds those used.
fn unique_name(name: String, taken: &mut std::collections::HashSet<String>) -> String {
    if taken.insert(ntfs_key(&name)) {
        return name;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name.as_str(), ""),
    };
    (2u32..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| taken.insert(ntfs_key(candidate)))
        .expect("a free number")
}

/// The copied items and everything below them, depth first with each folder's
/// names sorted; None when no item is a regular file or a folder.
pub fn host_files(tops: &[PathBuf]) -> Option<HostFiles> {
    let mut found: Vec<(String, PathBuf, bool)> = Vec::new();
    let mut bytes = 0u64;
    // Copied items of one name (from two folders, as in search results) stay apart, as do
    // the names in each folder below.
    let mut taken = std::collections::HashSet::new();
    let mut pending: Vec<(PathBuf, String)> = tops
        .iter()
        .filter_map(|top| Some((top.clone(), windows_name(top.file_name()?.to_str()?)?)))
        .map(|(top, name)| (top, unique_name(name, &mut taken)))
        .collect();
    pending.reverse();
    while let Some((path, rel)) = pending.pop() {
        let unreadable = |found: &Vec<_>| Some(HostFiles::Unreadable { entries: found.len() + 1, bytes });
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            // A copied item deleted since the copy is left out.
            if !rel.contains('\\') {
                continue;
            }
            return unreadable(&found);
        };
        if meta.is_dir() {
            let listed = std::fs::read_dir(&path)
                .and_then(|dir| dir.map(|entry| entry.map(|entry| entry.file_name())).collect::<std::io::Result<Vec<_>>>());
            let Ok(listed) = listed else {
                return unreadable(&found);
            };
            let mut names: Vec<(String, String)> = listed
                .into_iter()
                .filter_map(|name| name.into_string().ok())
                .filter_map(|name| Some((windows_name(&name)?, name)))
                .collect();
            names.sort();
            // Two Mac names may meet in Windows (`a:b` and a literal stand-in, or a case pair).
            let mut taken = std::collections::HashSet::new();
            let names: Vec<(String, String)> =
                names.into_iter().map(|(win, name)| (unique_name(win, &mut taken), name)).collect();
            pending.extend(names.iter().rev().map(|(win, name)| (path.join(name), format!("{rel}\\{win}"))));
        } else if meta.is_file() {
            bytes = bytes.saturating_add(meta.len());
        } else {
            continue;
        }
        found.push((rel, path, meta.is_dir()));
        if found.len() > CLIP_FILES_BIG_MAX_ENTRIES || bytes > CLIP_FILES_BIG_MAX_BYTES {
            return Some(HostFiles::TooLarge { entries: found.len(), bytes });
        }
    }
    if found.is_empty() {
        return None;
    }
    // Opened only now, so an unreadable file fails the copy here; one may have grown since.
    let (mut entries, mut bytes, mut digest) = (Vec::with_capacity(found.len()), 0u64, Sha256::new());
    for (rel, path, dir) in found {
        let file = if dir {
            None
        } else {
            let Some(version) = open_regular(&path) else {
                return Some(HostFiles::Unreadable { entries: entries.len() + 1, bytes });
            };
            bytes = bytes.saturating_add(version.0);
            if bytes > CLIP_FILES_BIG_MAX_BYTES {
                return Some(HostFiles::TooLarge { entries: entries.len() + 1, bytes });
            }
            Some(HostFileSource { path, version })
        };
        digest.update(rel.as_bytes());
        digest.update([0]);
        if let Some(file) = file.as_ref() {
            let (size, mtime, mtime_ns, ctime, ctime_ns) = file.version;
            digest.update(size.to_le_bytes());
            for time in [mtime, mtime_ns, ctime, ctime_ns] {
                digest.update(time.to_le_bytes());
            }
        }
        entries.push(HostFileEntry { rel, file });
    }
    Some(HostFiles::Read(HostFileCopy {
        entries,
        bytes,
        digest: digest.finalize().into(),
    }))
}

/// A regular file opened for reading (never through a link): its version.
fn open_regular(path: &Path) -> Option<SourceVersion> {
    open_source(path).ok().map(|(_, version)| version)
}

/// `path` opened for reading without following a link, when it is a regular file.
pub fn open_source(path: &Path) -> std::io::Result<(std::fs::File, SourceVersion)> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    Ok((file, source_version(&meta)))
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
