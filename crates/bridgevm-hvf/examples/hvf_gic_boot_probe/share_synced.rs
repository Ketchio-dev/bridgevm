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
//! one, whose guest copy is neither that nor the previous one and that is not marked is a guest
//! edit: it is fetched, not overwritten, unless the Mac file changes first. Anything else, an
//! unknown name, a marked name, a guest copy back at the previous version, a changed Mac file,
//! keeps the Mac winning as before.
//!
//! A confirmed transfer is not yet on the guest's disk: a PUT's data and the guest's own edit
//! (that a GET just copied) can sit in Windows' cache, and a force-stop or a probe kill then
//! brings back a cut-short copy with the guest's time. So a name stays marked for `SYNC_SETTLE`
//! after a confirmed PUT and after its fetched copy is published (marked on disk before the
//! publish). A fetch under way is not marked: a copy a restart cut off stays fetchable.

use super::*;
use std::time::{Duration, Instant};

/// How long a name stays marked after a PUT or a fetch completes: well past Windows' lazy writer
/// (dirty pages written within about a second, then every second) and the NTFS log flush, so the
/// guest copy both sides now share is on the guest's disk before it can be told from an edit.
pub const SYNC_SETTLE: Duration = Duration::from_secs(20);

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
    /// What an earlier session left: a loaded mark holds until this session syncs the name.
    pub fn load_synced(&mut self, files: HashMap<String, SyncedFile>, marks: HashSet<String>) {
        self.synced = files;
        self.marks = marks.into_iter().map(|name| (name, None)).collect();
    }

    /// Before a PUT of `name` goes out (the caller puts the mark on disk first).
    pub fn mark_put(&mut self, name: &str) {
        self.marks.insert(normalize_rel(name), None);
    }

    /// A PUT of `name` may follow soon (the caller marks it on disk with the PUT it is about to
    /// send, in one write): marked until `now + SYNC_SETTLE` unless it is marked already.
    pub fn mark_ahead(&mut self, name: &str, now: Instant) -> bool {
        let key = normalize_rel(name);
        if self.marks.contains_key(&key) {
            return false;
        }
        self.marks.insert(key, Some(now + SYNC_SETTLE));
        true
    }

    /// `name`'s copy is settling until `now + SYNC_SETTLE` (the caller has it marked on disk).
    pub fn mark_settling(&mut self, name: &str, now: Instant) {
        self.marks.insert(normalize_rel(name), Some(now + SYNC_SETTLE));
    }

    /// Settled marks leave (the next versions write drops them on disk): their names.
    pub fn expire_marks(&mut self, now: Instant) -> Vec<String> {
        let mut expired = Vec::new();
        self.marks.retain(|name, until| {
            let keep = until.is_none_or(|until| until > now);
            if !keep {
                expired.push(name.clone());
            }
            keep
        });
        self.synced_dirty |= !expired.is_empty();
        expired
    }

    #[cfg(test)]
    pub fn marked(&self, name: &str) -> bool {
        self.marks.contains_key(name)
    }

    /// A PUT of the Mac version (`size`, `mtime_ms`) confirmed: the guest holds it, with the
    /// Mac's time when one was sent (otherwise no version both sides share is known). It settles.
    pub fn put_confirmed(&mut self, name: &str, size: u64, mtime_ms: Option<u128>, now: Instant) {
        let key = normalize_rel(name);
        self.marks.insert(key.clone(), Some(now + SYNC_SETTLE));
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
            if self.records.contains_key(name) || self.marks.contains_key(name) {
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
