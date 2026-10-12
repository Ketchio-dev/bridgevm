//! Host-to-guest PUT outcomes. The guest agent truncates a file before writing
//! it, so from the moment a PUT is issued until its PUTOK the guest copy is
//! unknown and the record's hash no longer describes it: the next host scan
//! re-sends the host file even when its bytes and stat match the record (say
//! `git checkout` or `cp -p` restored them), and the guest copy is never pulled
//! back. A refused PUT is retried only once its host file changes, never every
//! tick.

use super::*;

impl ShareSync {
    pub fn on_put_ok(&mut self, name: String, size: u64, hash: u64) {
        let name = normalize_rel(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        self.put_rejected.remove(&name);
        self.records.insert(
            name,
            FileRecord {
                size,
                hash,
                host_mtime_ms: None,
                guest_mtime: None,
                awaiting_guest_stamp: true,
                host_write_failed: false,
                put_unconfirmed: false,
            },
        );
    }

    /// The guest refused a PUT (for example a name Windows rejects). Keep the
    /// scanned host version so it is not re-sent every tick ahead of LSR.
    pub fn on_put_rejected(&mut self, name: &str) {
        let name = normalize_rel(name);
        if let Some(stat) = self.host_files_scratch.get(&name).copied() {
            self.put_rejected.insert(name, stat);
        }
    }

    /// Windows keys, because the guest may store a held host name in another case.
    pub(super) fn pull_held_keys(&self) -> HashSet<String> {
        let unconfirmed = self
            .records
            .iter()
            .filter(|(_, record)| record.put_unconfirmed);
        (self.pending_host_changed.iter())
            .chain(self.put_rejected.keys())
            .chain(unconfirmed.map(|(name, _)| name))
            .map(|name| share_win_names::windows_key(name))
            .collect()
    }

    /// The host version is not on the guest yet (queued, refused or otherwise
    /// unconfirmed after the guest may have truncated its copy): never pull
    /// that copy back, nor keep it as a guest edit.
    pub(super) fn pull_held(&self, held: &HashSet<String>, name: &str) -> bool {
        !held.is_empty() && held.contains(&share_win_names::windows_key(name))
    }
}
