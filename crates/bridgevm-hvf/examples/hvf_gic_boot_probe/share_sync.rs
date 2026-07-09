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
}

pub struct ShareSync {
    records: HashMap<String, FileRecord>,
    max_bytes: u64,
    guest_skip_seen: HashSet<(String, String, SkipKey)>,
    pending_guest_mtime: HashMap<String, String>,
    pending_host_changed: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SkipKey {
    TooLarge,
}

impl ShareSync {
    pub fn new(max_kb: u64) -> Self {
        Self {
            records: HashMap::new(),
            max_bytes: max_kb.saturating_mul(1024),
            guest_skip_seen: HashSet::new(),
            pending_guest_mtime: HashMap::new(),
            pending_host_changed: HashSet::new(),
        }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn on_guest_listing(&mut self, entries: Vec<LsEntry>) -> Vec<SyncAction> {
        let mut actions = Vec::new();
        let mut present = HashSet::new();
        let mut file_entries = HashMap::new();

        for entry in entries {
            let name = normalize_rel(&entry.name);
            if name.is_empty() {
                continue;
            }
            present.insert(name.clone());
            if entry.is_dir {
                continue;
            }
            file_entries.insert(name, entry);
        }

        for (name, entry) in &file_entries {
            if entry.size > self.max_bytes {
                if self.remember_guest_skip(name, &entry.mtime, SkipKey::TooLarge) {
                    actions.push(SyncAction::Skip {
                        name: name.clone(),
                        reason: SkipReason::TooLarge { size: entry.size },
                    });
                }
                continue;
            }

            match self.records.get_mut(name) {
                None => {
                    self.pending_guest_mtime
                        .insert(name.clone(), entry.mtime.clone());
                    actions.push(SyncAction::Get { name: name.clone() });
                }
                Some(record) if record.awaiting_guest_stamp && entry.size == record.size => {
                    // A host PUT changes the guest mtime. The first matching LSR after
                    // PUTOK is our write landing, not a guest edit to pull back.
                    record.guest_mtime = Some(entry.mtime.clone());
                    record.awaiting_guest_stamp = false;
                }
                Some(record) if record.guest_mtime.as_deref() != Some(entry.mtime.as_str()) => {
                    self.pending_guest_mtime
                        .insert(name.clone(), entry.mtime.clone());
                    actions.push(SyncAction::Get { name: name.clone() });
                }
                Some(_) => {}
            }
        }

        for (name, record) in &self.records {
            if present.contains(name) {
                continue;
            }
            if record.hash == 0 {
                continue;
            }
            if self.pending_host_changed.contains(name) {
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
            if record.hash == hash {
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
            },
        );
        GuestFileOutcome::WriteHost(bytes)
    }

    pub fn note_host_stat(&mut self, name: &str, mtime_ms: u128) {
        let name = normalize_rel(name);
        if let Some(record) = self.records.get_mut(&name) {
            record.host_mtime_ms = Some(mtime_ms);
        }
    }

    pub fn on_host_scan(&mut self, files: Vec<HostFile>) -> Vec<SyncAction> {
        let mut actions = Vec::new();
        let mut present = HashSet::new();
        let mut host_files = HashMap::new();

        for file in files {
            let name = normalize_rel(&file.name);
            if name.is_empty() {
                continue;
            }
            present.insert(name.clone());
            host_files.insert(name, file);
        }

        for (name, file) in &host_files {
            if self.records.get(name).is_none_or(|record| {
                record.size != file.size || record.host_mtime_ms != Some(file.mtime_ms)
            }) {
                self.pending_host_changed.insert(name.clone());
                actions.push(SyncAction::Get { name: name.clone() });
            }
        }

        for (name, record) in &self.records {
            if present.contains(name) {
                continue;
            }
            if record.hash == 0 {
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
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        let hash = fnv1a64(&bytes);
        if let Some(record) = self.records.get_mut(&name) {
            if record.hash == hash {
                record.size = bytes.len() as u64;
                record.host_mtime_ms = Some(mtime_ms);
                return None;
            }
        }
        Some(PushGuest { bytes, hash })
    }

    pub fn on_put_ok(&mut self, name: String, size: u64, hash: u64) {
        let name = normalize_rel(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
        self.records.insert(
            name,
            FileRecord {
                size,
                hash,
                host_mtime_ms: None,
                guest_mtime: None,
                awaiting_guest_stamp: true,
            },
        );
    }

    pub fn on_guest_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
    }

    pub fn on_host_deleted(&mut self, name: &str) {
        let name = normalize_rel(name);
        self.records.remove(&name);
        self.pending_host_changed.remove(&name);
        self.pending_guest_mtime.remove(&name);
    }

    fn remember_guest_skip(&mut self, name: &str, mtime: &str, key: SkipKey) -> bool {
        self.guest_skip_seen
            .insert((name.to_string(), mtime.to_string(), key))
    }
}

/// Internal share keys are relative paths with forward slashes. Host joins on
/// macOS accept '/', while Windows guest paths are converted at the wire edge.
pub fn normalize_rel(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

pub fn to_guest_rel(name: &str) -> String {
    normalize_rel(name).replace('/', "\\")
}

pub fn from_guest_rel(name: &str) -> String {
    normalize_rel(name)
}

/// Parse the guest LS/LSR format `relpath|size|isDir|mtime`. The split is from
/// the right so a pathological file name containing `|` still round-trips; the
/// numeric fields and ISO mtime emitted by the guest agent never contain that
/// separator.
pub fn parse_ls(listing: &str) -> Vec<LsEntry> {
    listing
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.rsplitn(4, '|');
            let mtime = parts.next()?.to_string();
            let is_dir = parts.next()? == "1";
            let size = parts.next()?.parse().ok()?;
            let name = from_guest_rel(parts.next()?);
            Some(LsEntry {
                name,
                size,
                is_dir,
                mtime,
            })
        })
        .collect()
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ls_file(name: &str, size: u64, mtime: &str) -> LsEntry {
        LsEntry {
            name: name.into(),
            size,
            is_dir: false,
            mtime: mtime.into(),
        }
    }

    fn host_file(name: &str, size: u64, mtime_ms: u128) -> HostFile {
        HostFile {
            name: name.into(),
            size,
            mtime_ms,
        }
    }

    #[test]
    fn parse_ls_handles_lines_empty_pipe_names_and_normalizes_guest_rels() {
        assert!(parse_ls("").is_empty());
        let entries = parse_ls(
            "a.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
             sub\\dir|0|1|2026-01-01T00:00:01.0000000Z\n\
             sub\\odd|name.txt|4|0|2026-01-01T00:00:02.0000000Z\n",
        );
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].size, 3);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[1].name, "sub/dir");
        assert!(entries[1].is_dir);
        assert_eq!(entries[2].name, "sub/odd|name.txt");
    }

    #[test]
    fn rel_path_helpers_round_trip_guest_and_host_forms() {
        assert_eq!(from_guest_rel("sub\\dir\\file.txt"), "sub/dir/file.txt");
        assert_eq!(to_guest_rel("sub/dir/file.txt"), "sub\\dir\\file.txt");
        assert_eq!(
            from_guest_rel(&to_guest_rel("sub/dir/file.txt")),
            "sub/dir/file.txt"
        );
    }

    #[test]
    fn fnv1a_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn host_guest_round_trip_does_not_ping_pong() {
        let mut sync = ShareSync::new(512);
        assert_eq!(
            sync.on_host_scan(vec![host_file("sub/x.txt", 5, 10)]),
            vec![SyncAction::Get {
                name: "sub/x.txt".into()
            }]
        );
        let push = sync
            .on_host_file("sub/x.txt".into(), b"hello".to_vec(), 10)
            .expect("host edit pushes guest");
        sync.on_put_ok("sub/x.txt".into(), push.bytes.len() as u64, push.hash);

        let actions = sync.on_guest_listing(vec![ls_file("sub\\x.txt", 5, "guest-1")]);
        assert!(actions.is_empty(), "PUT landing mtime is only stamped");
        assert!(sync
            .on_guest_listing(vec![ls_file("sub\\x.txt", 5, "guest-1")])
            .is_empty());

        assert_eq!(
            sync.on_guest_listing(vec![ls_file("sub\\x.txt", 6, "guest-2")]),
            vec![SyncAction::Get {
                name: "sub/x.txt".into()
            }]
        );
        match sync.on_guest_file("sub\\x.txt".into(), b"world!".to_vec(), None) {
            GuestFileOutcome::WriteHost(bytes) => assert_eq!(bytes, b"world!"),
            GuestFileOutcome::AlreadySynced => panic!("guest edit must write host"),
        }
        sync.note_host_stat("sub/x.txt", 20);
        assert!(sync
            .on_host_file("sub/x.txt".into(), b"world!".to_vec(), 20)
            .is_none());
    }

    #[test]
    fn guest_oversize_skips_are_deduped_by_name_mtime_kind_and_dirs_are_ignored() {
        let mut sync = ShareSync::new(1);
        let entries = vec![
            ls_file("big.bin", 2048, "m1"),
            LsEntry {
                name: "sub".into(),
                size: 0,
                is_dir: true,
                mtime: "d1".into(),
            },
        ];
        assert_eq!(
            sync.on_guest_listing(entries.clone()),
            vec![SyncAction::Skip {
                name: "big.bin".into(),
                reason: SkipReason::TooLarge { size: 2048 },
            }]
        );
        assert!(sync.on_guest_listing(entries).is_empty());
        assert_eq!(
            sync.on_guest_listing(vec![ls_file("big.bin", 2048, "m2")]),
            vec![SyncAction::Skip {
                name: "big.bin".into(),
                reason: SkipReason::TooLarge { size: 2048 },
            }]
        );
    }

    #[test]
    fn recursive_host_scan_detects_nested_changes() {
        let mut sync = ShareSync::new(512);
        assert_eq!(
            sync.on_host_scan(vec![host_file("sub/dir/a.txt", 1, 10)]),
            vec![SyncAction::Get {
                name: "sub/dir/a.txt".into()
            }]
        );
        let push = sync
            .on_host_file("sub/dir/a.txt".into(), b"a".to_vec(), 10)
            .unwrap();
        sync.on_put_ok("sub/dir/a.txt".into(), 1, push.hash);
        sync.on_guest_listing(vec![ls_file("sub\\dir\\a.txt", 1, "g1")]);
        sync.note_host_stat("sub/dir/a.txt", 10);
        assert!(sync
            .on_host_scan(vec![host_file("sub/dir/a.txt", 1, 10)])
            .is_empty());
        assert_eq!(
            sync.on_host_scan(vec![host_file("sub/dir/a.txt", 2, 11)]),
            vec![SyncAction::Get {
                name: "sub/dir/a.txt".into()
            }]
        );
    }

    #[test]
    fn tombstone_lifecycle_host_delete_then_confirm_removes_record() {
        let mut sync = ShareSync::new(512);
        let push = sync
            .on_host_file("gone.txt".into(), b"gone".to_vec(), 10)
            .unwrap();
        sync.on_put_ok("gone.txt".into(), 4, push.hash);
        sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]);
        sync.note_host_stat("gone.txt", 10);

        assert_eq!(
            sync.on_host_scan(Vec::new()),
            vec![SyncAction::DeleteGuest {
                name: "gone.txt".into()
            }]
        );
        sync.on_guest_deleted("gone.txt");
        assert_eq!(
            sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]),
            vec![SyncAction::Get {
                name: "gone.txt".into()
            }]
        );
    }

    #[test]
    fn tombstone_lifecycle_guest_delete_then_confirm_removes_record() {
        let mut sync = ShareSync::new(512);
        let push = sync
            .on_host_file("gone.txt".into(), b"gone".to_vec(), 10)
            .unwrap();
        sync.on_put_ok("gone.txt".into(), 4, push.hash);
        sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]);
        sync.note_host_stat("gone.txt", 10);

        assert_eq!(
            sync.on_guest_listing(Vec::new()),
            vec![SyncAction::DeleteHost {
                name: "gone.txt".into()
            }]
        );
        sync.on_host_deleted("gone.txt");
        assert_eq!(
            sync.on_host_scan(vec![host_file("gone.txt", 4, 10)]),
            vec![SyncAction::Get {
                name: "gone.txt".into()
            }]
        );
    }

    #[test]
    fn never_recorded_path_absence_never_deletes() {
        let mut sync = ShareSync::new(512);
        assert!(sync.on_host_scan(Vec::new()).is_empty());
        assert!(sync.on_guest_listing(Vec::new()).is_empty());
    }

    #[test]
    fn modification_wins_when_host_deleted_but_guest_changed() {
        let mut sync = ShareSync::new(512);
        let push = sync
            .on_host_file("race.txt".into(), b"old".to_vec(), 10)
            .unwrap();
        sync.on_put_ok("race.txt".into(), 3, push.hash);
        sync.on_guest_listing(vec![ls_file("race.txt", 3, "g1")]);
        sync.note_host_stat("race.txt", 10);

        assert_eq!(
            sync.on_guest_listing(vec![ls_file("race.txt", 4, "g2")]),
            vec![SyncAction::Get {
                name: "race.txt".into()
            }]
        );
    }

    #[test]
    fn modification_wins_when_guest_deleted_but_host_changed() {
        let mut sync = ShareSync::new(512);
        let push = sync
            .on_host_file("race.txt".into(), b"old".to_vec(), 10)
            .unwrap();
        sync.on_put_ok("race.txt".into(), 3, push.hash);
        sync.on_guest_listing(vec![ls_file("race.txt", 3, "g1")]);
        sync.note_host_stat("race.txt", 10);

        assert_eq!(
            sync.on_host_scan(vec![host_file("race.txt", 4, 11)]),
            vec![SyncAction::Get {
                name: "race.txt".into()
            }]
        );
        assert!(sync.on_guest_listing(Vec::new()).is_empty());
    }
}
