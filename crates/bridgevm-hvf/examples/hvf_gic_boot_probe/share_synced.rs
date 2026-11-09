//! What synced across sessions, kept beside the VM's UEFI vars (agent_console/share/
//! share_sync_file.rs), so a new session can tell a guest edit from a stale guest copy without
//! comparing clocks.
//!
//! A new session knows no synced file, and its first listing let the Mac win for every name
//! both sides hold: a Windows edit the last session had not copied to the Mac yet (a large copy
//! a guest restart or a crash cut off) was overwritten by the older Mac copy. Each name keeps
//! its last and previous synced version (size and modification time, which both sides share
//! once synced, share_mtime.rs), and every PUT is marked before it can touch the guest copy
//! until it is confirmed. At the first listing, a name whose Mac copy is still the last synced
//! one, whose guest copy is neither that nor the previous one and that no PUT may have cut short
//! is a guest edit: it is fetched, not overwritten, unless the Mac file changes first. Anything
//! else, an unknown name, a marked PUT, a guest copy back at the previous version, a changed
//! Mac file, keeps the Mac winning as before.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncedVersion {
    pub size: u64,
    pub mtime_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncedFile {
    pub last: SyncedVersion,
    pub prev: Option<SyncedVersion>,
}

impl ShareSync {
    /// The state a previous session left (empty when there is none or it cannot be trusted).
    pub fn load_synced(&mut self, files: HashMap<String, SyncedFile>, marks: HashSet<String>) {
        self.synced = files;
        self.put_marks = marks;
    }

    /// Before a PUT of `name` goes out: true when the mark is new and must reach the disk first.
    pub fn mark_put(&mut self, name: &str) -> bool {
        let added = self.put_marks.insert(normalize_rel(name));
        self.synced_dirty |= added;
        added
    }

    /// Names whose PUT may have touched the guest copy without confirmation.
    pub fn put_marks(&self) -> &HashSet<String> {
        &self.put_marks
    }

    /// A PUT of the Mac version (`size`, `mtime_ms`) confirmed: the guest holds it whole, with
    /// the Mac's time when one was sent (otherwise no version both sides share is known).
    pub fn put_confirmed(&mut self, name: &str, size: u64, mtime_ms: Option<u128>) {
        let key = normalize_rel(name);
        self.synced_dirty |= self.put_marks.remove(&key);
        match mtime_ms {
            Some(mtime_ms) => self.note_synced(key, SyncedVersion { size, mtime_ms }),
            None => self.forget_synced(&key),
        }
    }

    pub(super) fn note_synced(&mut self, name: String, version: SyncedVersion) {
        let prev = match self.synced.get(&name) {
            Some(file) if file.last == version => return,
            Some(file) => Some(file.last),
            None => None,
        };
        let file = SyncedFile { last: version, prev };
        self.synced.insert(name.clone(), file);
        self.synced_changes.insert(name, Some(file));
        self.synced_dirty = true;
    }

    pub(super) fn forget_synced(&mut self, name: &str) {
        if self.synced.remove(name).is_some() {
            self.synced_changes.insert(name.to_string(), None);
            self.synced_dirty = true;
        }
    }

    /// The versions changed since the last call, when anything (marks too) changed.
    pub fn take_synced_changes(&mut self) -> Option<Vec<(String, Option<SyncedFile>)>> {
        if !std::mem::take(&mut self.synced_dirty) {
            return None;
        }
        Some(self.synced_changes.drain().collect())
    }

    /// The session's first applied listing (the wait ends before it is applied): a guest edit
    /// of a name whose Mac copy is the last synced version is fetched, not held for the Mac
    /// copy. A later listing without the name drops it, so the Mac file is sent again.
    pub(super) fn prefer_changed_guest_copies(&mut self) {
        let listed = &self.guest_file_entries_scratch;
        self.guest_changed.retain(|name, _| listed.contains_key(name));
        if self.listings > 0 {
            return;
        }
        for (name, entry) in listed {
            if self.records.contains_key(name) || self.put_marks.contains(name) {
                continue;
            }
            let (Some(host), Some(synced)) = (self.host_files_scratch.get(name), self.synced.get(name)) else {
                continue;
            };
            if (SyncedVersion { size: host.size, mtime_ms: host.mtime_ms }) != synced.last {
                continue;
            }
            let Some(mtime_ms) = share_mtime::guest_mtime_ms(&entry.mtime) else {
                continue;
            };
            let guest = SyncedVersion { size: entry.size, mtime_ms };
            if guest == synced.last || Some(guest) == synced.prev {
                continue;
            }
            self.pending_host_changed.remove(name);
            self.guest_changed.insert(name.clone(), *host);
        }
    }

    /// A guest edit preferred at the first listing: the Mac copy is not sent over it.
    pub(super) fn guest_copy_preferred(&self, name: &str) -> bool {
        self.guest_changed.contains_key(name)
    }
}
