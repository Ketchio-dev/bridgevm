//! Guest listing parsing and lexical share-name validation.

use super::{from_guest_rel, LsEntry};

/// Parse the guest LS/LSR format `relpath|size|isDir|mtime`. The split is from
/// the right so a pathological file name containing `|` still round-trips; the
/// numeric fields and ISO mtime emitted by the guest agent never contain that
/// separator.
#[cfg(test)]
pub(super) fn parse_ls(listing: &str) -> Vec<LsEntry> {
    let mut entries = Vec::new();
    parse_ls_into(listing, &mut entries);
    entries
}

pub fn parse_ls_into(listing: &str, out: &mut Vec<LsEntry>) {
    out.clear();
    for line in listing.lines().filter(|line| !line.is_empty()) {
        let mut parts = line.rsplitn(4, '|');
        let Some(mtime) = parts.next() else {
            continue;
        };
        let Some(is_dir) = parts.next() else {
            continue;
        };
        let Some(size) = parts.next().and_then(|part| part.parse().ok()) else {
            continue;
        };
        let Some(name) = parts.next().and_then(from_guest_rel) else {
            continue;
        };
        out.push(LsEntry {
            name,
            size,
            is_dir: is_dir == "1",
            mtime: mtime.to_string(),
        });
    }
}
