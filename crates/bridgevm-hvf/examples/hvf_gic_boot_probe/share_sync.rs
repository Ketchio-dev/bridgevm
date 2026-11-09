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
    Skip(SkipReason),
}

/// `on_guest_version`'s answer: the bytes stay with the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuestVersion {
    AlreadySynced,
    WriteHost,
    Skip(SkipReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushGuest {
    pub bytes: Vec<u8>,
    pub hash: u64,
}

#[derive(Debug, Clone)]
struct FileRecord {
    size: u64,
    /// None for a large file recorded from its size and time alone (share_adopt.rs).
    hash: Option<u64>,
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
    /// The largest file an agent without ranged transfers moves (whole in memory).
    whole_file_limit: Option<u64>,
    guest_skip_seen: HashSet<(String, String, SkipKey)>,
    pending_guest_mtime: HashMap<String, String>,
    pending_host_changed: HashSet<String>,
    present_scratch: HashSet<String>,
    guest_file_entries_scratch: HashMap<String, GuestFileStat>,
    host_files_scratch: HashMap<String, HostFileStat>,
    put_rejected: HashMap<String, HostFileStat>,
    host_unseen: HashSet<String>,
    host_aliased: HashSet<String>,
    refused_names_seen: (usize, u64),
    fetched_over_cap: HashMap<String, String>,
    refused_writes: share_write_failures::RefusedWrites,
    /// This session has had a guest listing reply, even a refused one (share_adopt.rs).
    listed: bool,
    /// Guest listings applied this session; only the first one is adopted from.
    listings: u32,
    /// Synced versions and unconfirmed PUTs across sessions (share_synced.rs).
    synced: HashMap<String, share_synced::SyncedFile>,
    /// Marked names: an unconfirmed PUT or a loaded mark (None), or a settling copy (until when).
    marks: HashMap<String, Option<std::time::Instant>>,
    synced_changes: HashMap<String, Option<share_synced::SyncedFile>>,
    synced_dirty: bool,
    /// Guest edits preferred at the first listing, with the Mac file's stat then.
    guest_changed: HashMap<String, HostFileStat>,
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

    /// The share's size cap, lowered to `whole_file_limit` while the agent moves files whole.
    pub fn max_bytes(&self) -> u64 {
        self.whole_file_limit
            .map_or(self.max_bytes, |limit| limit.min(self.max_bytes))
    }

    /// An agent without ranged transfers (READY without `bigfile`) gets files of at most `limit`.
    pub fn set_whole_file_limit(&mut self, limit: Option<u64>) {
        self.whole_file_limit = limit;
    }

    /// The size and time the last guest listing gave `name`.
    pub fn listed_file(&self, name: &str) -> Option<(u64, &str)> {
        let entry = self.guest_file_entries_scratch.get(name)?;
        Some((entry.size, entry.mtime.as_str()))
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
        self.prefer_changed_guest_copies();
        let held = self.pull_held_keys();
        self.drop_fetched_over_cap();
        self.drop_refused_writes();

        let max_bytes = self.max_bytes();
        for (name, entry) in &self.guest_file_entries_scratch {
            if entry.size > max_bytes {
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
        self.retain_guest_skips();

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
        self.listed = true;
        self.listings = self.listings.saturating_add(1);

        actions
    }

    pub fn on_guest_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        mtime: Option<&str>,
    ) -> GuestFileOutcome {
        let size = bytes.len() as u64;
        match self.on_guest_version(name, size, fnv1a64(&bytes), mtime) {
            GuestVersion::AlreadySynced => GuestFileOutcome::AlreadySynced,
            GuestVersion::Skip(reason) => GuestFileOutcome::Skip(reason),
            GuestVersion::WriteHost => GuestFileOutcome::WriteHost(bytes),
        }
    }

    /// A fetched guest file known by its size and hash (a large one streams to the Mac
    /// without being held whole): whether the host copy must be written.
    pub fn on_guest_version(
        &mut self,
        name: String,
        size: u64,
        hash: u64,
        mtime: Option<&str>,
    ) -> GuestVersion {
        let name = normalize_rel(&name);
        if size > self.max_bytes() {
            return GuestVersion::Skip(self.refuse_over_cap(name, size, mtime));
        }
        let guest_mtime = mtime
            .map(str::to_string)
            .or_else(|| self.pending_guest_mtime.remove(&name));
        self.pending_host_changed.remove(&name);
        self.guest_changed.remove(&name);
        if let Some(record) = self.records.get_mut(&name) {
            if !record.host_write_failed && record.hash == Some(hash) {
                record.size = size;
                record.guest_mtime = guest_mtime;
                record.awaiting_guest_stamp = false;
                return GuestVersion::AlreadySynced;
            }
        }
        self.records.insert(
            name,
            FileRecord {
                size,
                hash: Some(hash),
                host_mtime_ms: None,
                guest_mtime,
                awaiting_guest_stamp: false,
                host_write_failed: true,
                put_unconfirmed: false,
            },
        );
        GuestVersion::WriteHost
    }

    #[cfg(test)]
    pub fn note_host_stat(&mut self, name: &str, mtime_ms: u128) {
        self.on_host_write_succeeded(name, Some(mtime_ms));
    }

    pub fn on_host_write_succeeded(&mut self, name: &str, mtime_ms: Option<u128>) {
        let name = normalize_rel(name);
        let Some(record) = self.records.get_mut(&name) else {
            return;
        };
        record.host_write_failed = false;
        record.host_mtime_ms = mtime_ms;
        // Both sides now hold this version (the Mac copy keeps the guest's time).
        let size = record.size;
        if let Some(mtime_ms) = mtime_ms {
            self.note_synced(name, share_synced::SyncedVersion { size, mtime_ms });
        }
    }

    /// No file has synced either way yet, so an empty guest listing deletes nothing.
    pub fn has_synced(&self) -> bool {
        !self.records.is_empty()
    }

    /// The listed guest time of a file this engine knows.
    pub fn guest_mtime(&self, name: &str) -> Option<&str> {
        self.records.get(&normalize_rel(name))?.guest_mtime.as_deref()
    }

    pub fn on_guest_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        // The listed copy is gone: it must never be adopted (share_adopt.rs).
        self.guest_file_entries_scratch.remove(&name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        self.forget_synced(&name);
    }

    pub fn on_host_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        self.guest_file_entries_scratch.remove(&name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        self.forget_synced(&name);
    }
}

#[path = "share_rel_path.rs"]
mod share_rel_path;
#[cfg(test)]
pub use share_rel_path::to_guest_rel;
pub use share_rel_path::{append_guest_rel_into, from_guest_rel, normalize_rel};
#[path = "share_guest_names.rs"]
mod share_guest_names;
pub use share_guest_names::{composed, guest_char, is_stand_in, key_from_guest};

#[path = "share_hash.rs"]
mod share_hash;
#[path = "share_mtime.rs"]
pub mod share_mtime;
#[path = "share_adopt.rs"]
mod share_adopt;
#[path = "share_synced.rs"]
pub mod share_synced;
pub use share_hash::{fnv1a64, fnv1a64_continue, FNV1A64_START};
#[path = "share_host_unseen.rs"]
mod share_host_unseen;
#[path = "share_listing.rs"]
mod share_listing;
#[path = "share_name_limits.rs"]
mod share_name_limits;
#[path = "share_name_refusals.rs"]
mod share_name_refusals;
pub use share_name_refusals::RefusedNames;
#[path = "share_put_outcome.rs"]
mod share_put_outcome;
#[path = "share_size_cap.rs"]
mod share_size_cap;
#[path = "share_sync_host.rs"]
mod share_sync_host;
#[path = "share_win_alias.rs"]
mod share_win_alias;
#[path = "share_win_names.rs"]
mod share_win_names;
pub use share_win_names::windows_key;
#[path = "share_write_failures.rs"]
mod share_write_failures;
#[cfg(test)]
use share_listing::parse_ls;
pub use share_listing::parse_ls_into;
pub use share_listing::ListingLimitError;
pub use share_listing::MAX_LISTING_BYTES;
pub use share_listing::MAX_LISTING_ENTRIES;

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
