//! A new session (every VM start or guest restart) knows of no synced file, so
//! it sent every Mac file to the guest again: the whole guest folder rewritten
//! at each start. Both sides keep a file's modification time (share_mtime.rs),
//! so a Mac file without a record now waits for the session's first guest
//! listing. A guest copy listed with the same size and time is recorded as
//! synced instead of sent. Any other file is sent as before, and the Mac keeps
//! winning for a file both sides hold: the waiting name is held from that
//! listing like any changed host file.
//!
//! Only that first listing is adopted from. The service lists again only once
//! no host change is left to send, so the first sync pass ends before a second
//! listing replaces it. Later, a listed copy may since have been deleted (a
//! Mac file deleted and restored with its time), and adopting it would make
//! the next listing delete the restored Mac file. A confirmed deletion also
//! drops the name's listed entry, for the same case within the first pass.

use super::*;

impl ShareSync {
    /// An unrecorded Mac file waits for this session's first guest listing.
    pub fn waits_for_first_listing(&self, name: &str) -> bool {
        !self.listed && !self.records.contains_key(name)
    }

    /// A refused or failed listing ends the wait too: nothing is recorded,
    /// so every file is sent, as before.
    pub fn end_first_listing_wait(&mut self) {
        self.listed = true;
    }

    /// Record the Mac file `name` (as read: `bytes`, `mtime_ms`) as synced
    /// when the last listing holds a guest copy of that size and time.
    pub fn adopt_listed_copy(&mut self, name: &str, bytes: &[u8], mtime_ms: u128) -> bool {
        let size = bytes.len() as u64;
        if self.listings != 1 || self.records.contains_key(name) {
            return false;
        }
        let Some(entry) = self.guest_file_entries_scratch.get(name) else {
            return false;
        };
        if entry.size != size || share_mtime::guest_mtime_ms(&entry.mtime) != Some(mtime_ms) {
            return false;
        }
        let guest_mtime = Some(entry.mtime.clone());
        let hash = fnv1a64(bytes);
        self.pending_host_changed.remove(name);
        self.pending_guest_mtime.remove(name);
        self.records.insert(
            name.to_string(),
            FileRecord {
                size,
                hash,
                host_mtime_ms: Some(mtime_ms),
                guest_mtime,
                awaiting_guest_stamp: false,
                host_write_failed: false,
                put_unconfirmed: false,
            },
        );
        true
    }
}
