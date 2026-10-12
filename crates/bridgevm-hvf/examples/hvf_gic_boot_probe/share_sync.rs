use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub mtime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFile {
    pub name: String,
    pub size: u64,
    pub mtime_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Get { name: String },
    DeleteGuest { name: String },
    DeleteHost { name: String },
    Skip { name: String, reason: SkipReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    TooLarge { size: u64 },
    WindowsAlias(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestFileOutcome {
    AlreadySynced,
    WriteHost(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushGuest {
    pub bytes: Vec<u8>,
    pub hash: u64,
}

#[derive(Debug, Clone)]
struct FileRecord {
    size: u64,
    hash: u64,
    host_mtime_ms: Option<u128>,
    guest_mtime: Option<String>,
    awaiting_guest_stamp: bool,
    host_write_failed: bool,
    put_unconfirmed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostFileStat {
    size: u64,
    mtime_ms: u128,
}

#[derive(Debug, Clone)]
struct GuestFileStat {
    size: u64,
    mtime: String,
}

#[derive(Default)]
pub struct ShareSync {
    records: HashMap<String, FileRecord>,
    max_bytes: u64,
    guest_skip_seen: HashSet<(String, String, SkipKey)>,
    pending_guest_mtime: HashMap<String, String>,
    pending_host_changed: HashSet<String>,
    present_scratch: HashSet<String>,
    guest_file_entries_scratch: HashMap<String, GuestFileStat>,
    host_files_scratch: HashMap<String, HostFileStat>,
    put_rejected: HashMap<String, HostFileStat>,
    host_unseen: HashSet<String>,
    host_aliased: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SkipKey {
    TooLarge,
}

impl ShareSync {
    pub fn new(max_kb: u64) -> Self {
        Self {
            max_bytes: max_kb.saturating_mul(1024),
            ..Self::default()
        }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    #[cfg(test)]
    pub fn on_guest_listing<I>(&mut self, entries: I) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = LsEntry>,
    {
        self.on_guest_listing_with(entries, true)
    }

    pub fn on_guest_listing_normalized<I>(&mut self, entries: I) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = LsEntry>,
    {
        self.on_guest_listing_with(entries, false)
    }

    fn on_guest_listing_with<I>(&mut self, entries: I, normalize_names: bool) -> Vec<SyncAction>
    where
        I: IntoIterator<Item = LsEntry>,
    {
        let mut actions = Vec::new();
        self.present_scratch.clear();
        self.guest_file_entries_scratch.clear();

        for entry in entries {
            let LsEntry {
                name,
                size,
                is_dir,
                mtime,
            } = entry;
            let name = if normalize_names {
                normalize_rel(&name)
            } else {
                name
            };
            if name.is_empty() {
                continue;
            }
            self.present_scratch.insert(name.clone());
            if is_dir {
                continue;
            }
            self.guest_file_entries_scratch
                .insert(name, GuestFileStat { size, mtime });
        }
        self.alias_guest_renames();
        let held = self.pull_held_keys();

        for (name, entry) in &self.guest_file_entries_scratch {
            if entry.size > self.max_bytes {
                if self.guest_skip_seen.insert((
                    name.clone(),
                    entry.mtime.clone(),
                    SkipKey::TooLarge,
                )) {
                    actions.push(SyncAction::Skip {
                        name: name.clone(),
                        reason: SkipReason::TooLarge { size: entry.size },
                    });
                }
                continue;
            }
            if self.pull_held(&held, name) {
                continue;
            }
            let changed = match self.records.get_mut(name) {
                Some(record) if record.awaiting_guest_stamp && entry.size == record.size => {
                    // A host PUT changes the guest mtime. The first matching LSR after
                    // PUTOK is our write landing, not a guest edit to pull back.
                    record.guest_mtime = Some(entry.mtime.clone());
                    record.awaiting_guest_stamp = false;
                    false
                }
                Some(record) => record.guest_mtime.as_deref() != Some(entry.mtime.as_str()),
                None => true,
            };
            if changed {
                // Noted even for a held host name, so a hold ending with the
                // host name gone fetches this edit instead of deleting it.
                self.pending_guest_mtime
                    .insert(name.clone(), entry.mtime.clone());
                if !self.host_held(name) {
                    actions.push(SyncAction::Get { name: name.clone() });
                }
            }
        }
        // A skip is reported once per listed oversized version. Forget versions
        // that left the listing, or guest listings grow this set without limit.
        let listed = &self.guest_file_entries_scratch;
        let max_bytes = self.max_bytes;
        self.guest_skip_seen.retain(|(name, mtime, _)| {
            listed
                .get(name)
                .is_some_and(|entry| entry.size > max_bytes && entry.mtime == *mtime)
        });

        for (name, record) in &self.records {
            if self.present_scratch.contains(name) {
                continue;
            }
            if record.host_write_failed {
                continue;
            }
            if self.pending_host_changed.contains(name) || self.host_held(name) {
                continue;
            }
            actions.push(SyncAction::DeleteHost { name: name.clone() });
        }

        actions
    }

    pub fn on_guest_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        mtime: Option<&str>,
    ) -> GuestFileOutcome {
        let name = normalize_rel(&name);
        let hash = fnv1a64(&bytes);
        let size = bytes.len() as u64;
        let guest_mtime = mtime
            .map(str::to_string)
            .or_else(|| self.pending_guest_mtime.remove(&name));
        self.pending_host_changed.remove(&name);
        if let Some(record) = self.records.get_mut(&name) {
            if !record.host_write_failed && record.hash == hash {
                record.size = size;
                record.guest_mtime = guest_mtime;
                record.awaiting_guest_stamp = false;
                return GuestFileOutcome::AlreadySynced;
            }
        }
        self.records.insert(
            name,
            FileRecord {
                size,
                hash,
                host_mtime_ms: None,
                guest_mtime,
                awaiting_guest_stamp: false,
                host_write_failed: true,
                put_unconfirmed: false,
            },
        );
        GuestFileOutcome::WriteHost(bytes)
    }

    #[cfg(test)]
    pub fn note_host_stat(&mut self, name: &str, mtime_ms: u128) {
        self.on_host_write_succeeded(name, Some(mtime_ms));
    }

    pub fn on_host_write_succeeded(&mut self, name: &str, mtime_ms: Option<u128>) {
        if let Some(record) = self.records.get_mut(&normalize_rel(name)) {
            record.host_write_failed = false;
            record.host_mtime_ms = mtime_ms;
        }
    }

    pub fn forget_absent_failed(&mut self, mut is_absent: impl FnMut(&str) -> bool) {
        let guest = &self.present_scratch;
        self.records.retain(|name, record| {
            !(record.host_write_failed && !guest.contains(name) && is_absent(name))
        });
        self.pending_guest_mtime.retain(|name, _| guest.contains(name));
        self.pending_host_changed.retain(|name| self.records.contains_key(name));
    }

    pub fn on_guest_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
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
        let (count, bytes) = self.records.iter().filter(|(_, r)| r.host_write_failed)
            .fold((0usize, 0usize), |(count, bytes), (name, _)|
                (count.saturating_add(1), bytes.saturating_add(name.len())));
        count >= 4096 || bytes >= 4 * 1024 * 1024
    }

    pub fn on_host_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
    }
}

#[path = "share_rel_path.rs"]
mod share_rel_path;
#[cfg(test)]
pub use share_rel_path::to_guest_rel;
pub use share_rel_path::{append_guest_rel_into, from_guest_rel, normalize_rel};

#[path = "share_host_unseen.rs"]
mod share_host_unseen;
#[path = "share_listing.rs"]
mod share_listing;
#[path = "share_put_outcome.rs"]
mod share_put_outcome;
#[path = "share_sync_host.rs"]
mod share_sync_host;
#[path = "share_win_alias.rs"]
mod share_win_alias;
#[path = "share_win_names.rs"]
mod share_win_names;
#[cfg(test)]
use share_listing::parse_ls;
pub use share_listing::parse_ls_into;

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "share_sync_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "share_sync_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "share_sync_failure_budget_tests.rs"]
mod failure_budget_tests;
#[cfg(test)]
#[path = "share_sync_windows_name_tests.rs"]
mod windows_name_tests;
