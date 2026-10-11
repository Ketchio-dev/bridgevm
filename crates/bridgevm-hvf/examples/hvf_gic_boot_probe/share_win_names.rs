//! Case-only renames. NTFS looks names up case-insensitively and Win32 drops
//! trailing dots and spaces from the last component, so a renamed name can
//! still be the same file on the other side; treat it as that file.

use super::*;

/// Simple per-character upper-case fold (BMP only, one-to-one mappings only)
/// with trailing dots and spaces trimmed from the last component.
pub fn windows_key(name: &str) -> String {
    let fold = |c: char| {
        let mut upper = c.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(u), None) if u32::from(c) <= 0xFFFF && u32::from(u) <= 0xFFFF => u,
            _ => c,
        }
    };
    name.trim_end_matches(['.', ' ']).chars().map(fold).collect()
}

/// Windows key -> the only name with that key (None when several share it).
fn unique_by_key<'a>(names: impl Iterator<Item = &'a String>) -> HashMap<String, Option<&'a String>> {
    let mut map = HashMap::new();
    for name in names {
        map.entry(windows_key(name))
            .and_modify(|slot| *slot = None)
            .or_insert(Some(name));
    }
    map
}

/// Pairs a listed name that has no record with the record whose file is no
/// longer listed, when each is the only name with that key on its side.
fn renamed_pairs<'a>(
    listed: impl Iterator<Item = &'a String>,
    records: &'a HashMap<String, FileRecord>,
    is_listed: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let recorded = unique_by_key(records.keys());
    let mut pairs = Vec::new();
    for (key, name) in unique_by_key(listed) {
        let Some(name) = name.filter(|name| !records.contains_key(*name)) else {
            continue;
        };
        let Some(Some(old)) = recorded.get(&key) else {
            continue;
        };
        if !is_listed(old) && !records[*old].host_write_failed {
            pairs.push((name.clone(), (*old).clone()));
        }
    }
    pairs
}

impl ShareSync {
    /// The record key follows the host spelling: a host-side case rename moves
    /// the record instead of uploading a copy and deleting the guest original.
    pub(super) fn adopt_host_renames(&mut self) {
        let host = &self.host_files_scratch;
        let records = &self.records;
        if !host.keys().any(|name| !records.contains_key(name))
            || !records.keys().any(|name| !host.contains_key(name))
        {
            return;
        }
        for (new, old) in renamed_pairs(host.keys(), records, |name| host.contains_key(name)) {
            if let Some(record) = self.records.remove(&old) {
                self.records.insert(new.clone(), record);
            }
            if let Some(mtime) = self.pending_guest_mtime.remove(&old) {
                self.pending_guest_mtime.insert(new.clone(), mtime);
            }
            if self.pending_host_changed.remove(&old) {
                self.pending_host_changed.insert(new);
            }
        }
    }

    /// The guest keeps its own spelling: list it under the record's name, so
    /// a guest-side case rename neither deletes nor re-fetches the file.
    pub(super) fn alias_guest_renames(&mut self) {
        let listed = &self.guest_file_entries_scratch;
        let present = &self.present_scratch;
        let records = &self.records;
        if !listed.keys().any(|name| !records.contains_key(name))
            || !records.keys().any(|name| !present.contains(name))
        {
            return;
        }
        for (name, record) in renamed_pairs(listed.keys(), records, |name| present.contains(name)) {
            if let Some(entry) = self.guest_file_entries_scratch.remove(&name) {
                self.guest_file_entries_scratch.insert(record.clone(), entry);
            }
            self.present_scratch.remove(&name);
            self.present_scratch.insert(record);
        }
    }
}
