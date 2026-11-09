//! Host-scan planning: changed host files become PUTs and vanished ones guest
//! deletions. A case-only rename is the same file; a refused PUT waits for a
//! host change (`share_put_outcome`); a file the scan found but could not read
//! has not vanished; names Windows cannot keep apart are held, neither
//! uploaded nor vanished.

use super::*;

impl ShareSync {
    #[cfg(test)]
    pub fn on_host_scan<I>(&mut self, files: I) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = HostFile>,
    {
        self.on_host_scan_with(files, true)
    }

    pub fn on_host_scan_normalized<I>(&mut self, files: I) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = HostFile>,
    {
        self.on_host_scan_with(files, false)
    }

    fn on_host_scan_with<I>(&mut self, files: I, normalize_names: bool) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = HostFile>,
    {
        let mut actions = Vec::new();
        self.host_files_scratch.clear();

        for file in files {
            let HostFile {
                name,
                size,
                mtime_ms,
            } = file;
            let name = if normalize_names {
                normalize_rel(&name)
            } else {
                name
            };
            if name.is_empty() {
                continue;
            }
            self.host_files_scratch
                .insert(name, HostFileStat { size, mtime_ms });
        }
        self.hold_windows_aliases(&mut actions);

        for (name, file) in &self.host_files_scratch {
            if self.records.get(name).is_some_and(|record| record.host_write_failed) {
                continue;
            }
            // An unconfirmed PUT is re-sent even if the host stat is the synced one.
            if self.records.get(name).is_none_or(|record| {
                record.size != file.size
                    || record.host_mtime_ms != Some(file.mtime_ms)
                    || record.put_unconfirmed
            }) {
                self.pending_host_changed.insert(name.clone());
                // A PUT the guest refused is retried once the host file changes.
                if self.put_rejected.get(name) != Some(file) {
                    actions.push(SyncAction::Get { name: name.clone() });
                }
            }
        }
        self.retain_put_rejected();

        for (name, record) in &self.records {
            if self.host_files_scratch.contains_key(name) || self.host_held(name) {
                continue;
            }
            if record.host_write_failed {
                continue;
            }
            if self.pending_guest_mtime.contains_key(name) {
                continue;
            }
            actions.push(SyncAction::DeleteGuest { name: name.clone() });
        }

        actions
    }

    pub fn on_host_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        mtime_ms: u128,
    ) -> Option<PushGuest> {
        let name = normalize_rel(&name);
        if self.records.get(&name).is_some_and(|record| record.host_write_failed) {
            return None;
        }
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        let hash = fnv1a64(&bytes);
        if let Some(record) = self.records.get_mut(&name) {
            if record.hash == Some(hash) && !record.put_unconfirmed {
                record.size = bytes.len() as u64;
                record.host_mtime_ms = Some(mtime_ms);
                return None;
            }
            // The guest truncates before writing: its copy is unknown until PUTOK.
            record.put_unconfirmed = true;
        }
        // For the same reason the listed copy may be gone: never adopt it (share_adopt.rs).
        self.guest_file_entries_scratch.remove(&name);
        Some(PushGuest { bytes, hash })
    }

    /// A large host file streams to the guest without being read first, so it has no hash
    /// to compare yet: it is sent whenever the scan found it changed (on_put_ok records it).
    pub fn on_host_push_streamed(&mut self, name: &str) -> bool {
        let name = normalize_rel(name);
        if self.records.get(&name).is_some_and(|record| record.host_write_failed) {
            return false;
        }
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        if let Some(record) = self.records.get_mut(&name) {
            record.put_unconfirmed = true;
        }
        self.guest_file_entries_scratch.remove(&name);
        true
    }
}
