//! Failed guest-to-host writes. A failed write never becomes a reverse upload
//! or a guest deletion, and it is retried at the next guest listing, unless the
//! guest write policy refused it for good (follow-up of 2026-10-23). Then the
//! file is not fetched again while the guest lists it with the size and mtime
//! it was refused at; a new size or mtime, or the file leaving the listing,
//! forgets the refusal. At most MAX_REFUSED_WRITES refusals are kept and the
//! oldest is forgotten first, so it is fetched again. The table is held in
//! memory only: a restarted probe fetches every file once more.

use super::*;

/// Refusals remembered at most; the oldest is forgotten first.
const MAX_REFUSED_WRITES: usize = 1024;

/// Guest path -> the refused version's size, the FNV-1a hash of its listed
/// mtime (the guest picks that string, so it is not kept), and the order in
/// which it was refused.
#[derive(Default)]
pub(super) struct RefusedWrites {
    versions: HashMap<String, (u64, u64, u64)>,
    refused: u64,
}

impl RefusedWrites {
    fn remember(&mut self, name: String, size: u64, mtime: u64) {
        if self.versions.len() >= MAX_REFUSED_WRITES && !self.versions.contains_key(&name) {
            let oldest = self.versions.iter().min_by_key(|(_, version)| version.2);
            if let Some(oldest) = oldest.map(|(name, _)| name.clone()) {
                self.versions.remove(&oldest);
            }
        }
        self.refused += 1;
        self.versions.insert(name, (size, mtime, self.refused));
    }
}

impl ShareSync {
    pub fn forget_absent_failed(&mut self, mut is_absent: impl FnMut(&str) -> bool) {
        let guest = &self.present_scratch;
        self.records.retain(|name, record| {
            !(record.host_write_failed && !guest.contains(name) && is_absent(name))
        });
        self.pending_guest_mtime
            .retain(|name, _| guest.contains(name));
        self.pending_host_changed
            .retain(|name| self.records.contains_key(name));
    }

    /// Retry a failed write on the next guest listing. Never turn its missing,
    /// old or partial destination into a reverse upload or a guest deletion.
    pub fn on_host_write_failed(&mut self, name: &str) -> bool {
        let name = normalize_rel(name);
        self.pending_host_changed.remove(&name);
        if let Some(record) = self.records.get_mut(&name) {
            record.host_write_failed = true;
            record.guest_mtime = None;
        }
        // Do not evict failed-destination protection to accommodate guest churn.
        // The caller disables sync at this bound rather than risking data loss.
        let (count, bytes) = self
            .records
            .iter()
            .filter(|(_, r)| r.host_write_failed)
            .fold((0usize, 0usize), |(count, bytes), (name, _)| {
                (count.saturating_add(1), bytes.saturating_add(name.len()))
            });
        count >= 4096 || bytes >= 4 * 1024 * 1024
    }

    /// A failed write of `name`, `persistent` when the guest write policy
    /// refused it for good; true when the failed-destination limit is reached.
    pub fn on_host_write_error(&mut self, name: &str, persistent: bool) -> bool {
        let key = normalize_rel(name);
        let refused = self.records.get(&key).and_then(|record| {
            let mtime = record.guest_mtime.as_deref()?;
            Some((record.size, fnv1a64(mtime.as_bytes())))
        });
        if let Some((size, mtime)) = refused.filter(|_| persistent) {
            self.refused_writes.remember(key, size, mtime);
        }
        self.on_host_write_failed(name)
    }

    /// Take the refused versions out of this listing's candidates, and forget
    /// refusals whose file changed size or mtime or left the listing.
    pub(super) fn drop_refused_writes(&mut self) {
        let listed = &mut self.guest_file_entries_scratch;
        self.refused_writes
            .versions
            .retain(|name, (size, mtime, _)| {
                listed.get(name).is_some_and(|entry| {
                    entry.size == *size && fnv1a64(entry.mtime.as_bytes()) == *mtime
                })
            });
        for name in self.refused_writes.versions.keys() {
            listed.remove(name);
        }
    }
}
