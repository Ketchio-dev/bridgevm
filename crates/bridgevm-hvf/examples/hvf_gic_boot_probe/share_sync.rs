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
    Skip { name: String, reason: SkipReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Dir,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SkipKey {
    Dir,
    TooLarge,
}

impl ShareSync {
    pub fn new(max_kb: u64) -> Self {
        Self {
            records: HashMap::new(),
            max_bytes: max_kb.saturating_mul(1024),
            guest_skip_seen: HashSet::new(),
            pending_guest_mtime: HashMap::new(),
        }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn on_guest_listing(&mut self, entries: Vec<LsEntry>) -> Vec<SyncAction> {
        let mut actions = Vec::new();
        for entry in entries {
            if entry.is_dir {
                if self.remember_guest_skip(&entry.name, &entry.mtime, SkipKey::Dir) {
                    actions.push(SyncAction::Skip {
                        name: entry.name,
                        reason: SkipReason::Dir,
                    });
                }
                continue;
            }
            if entry.size > self.max_bytes {
                if self.remember_guest_skip(&entry.name, &entry.mtime, SkipKey::TooLarge) {
                    actions.push(SyncAction::Skip {
                        name: entry.name,
                        reason: SkipReason::TooLarge { size: entry.size },
                    });
                }
                continue;
            }

            match self.records.get_mut(&entry.name) {
                None => {
                    self.pending_guest_mtime
                        .insert(entry.name.clone(), entry.mtime.clone());
                    actions.push(SyncAction::Get { name: entry.name });
                }
                Some(record) if record.awaiting_guest_stamp && entry.size == record.size => {
                    // A host PUT changes the guest mtime. The first matching LS after
                    // PUTOK is our write landing, not a guest edit to pull back.
                    record.guest_mtime = Some(entry.mtime);
                    record.awaiting_guest_stamp = false;
                }
                Some(record) if record.guest_mtime.as_deref() != Some(entry.mtime.as_str()) => {
                    self.pending_guest_mtime
                        .insert(entry.name.clone(), entry.mtime.clone());
                    actions.push(SyncAction::Get { name: entry.name });
                }
                Some(_) => {}
            }
        }
        actions
    }

    pub fn on_guest_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        mtime: Option<&str>,
    ) -> GuestFileOutcome {
        let hash = fnv1a64(&bytes);
        let size = bytes.len() as u64;
        let guest_mtime = mtime
            .map(str::to_string)
            .or_else(|| self.pending_guest_mtime.remove(&name));
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
        if let Some(record) = self.records.get_mut(name) {
            record.host_mtime_ms = Some(mtime_ms);
        }
    }

    pub fn on_host_scan(&mut self, files: Vec<HostFile>) -> Vec<String> {
        files
            .into_iter()
            .filter(|file| {
                self.records.get(&file.name).is_none_or(|record| {
                    record.size != file.size || record.host_mtime_ms != Some(file.mtime_ms)
                })
            })
            .map(|file| file.name)
            .collect()
    }

    pub fn on_host_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        mtime_ms: u128,
    ) -> Option<PushGuest> {
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

    fn remember_guest_skip(&mut self, name: &str, mtime: &str, key: SkipKey) -> bool {
        self.guest_skip_seen
            .insert((name.to_string(), mtime.to_string(), key))
    }
}

/// Parse the guest LS format `name|size|isDir|mtime`. The split is from the
/// right so a pathological file name containing `|` still round-trips; the
/// numeric fields and ISO mtime emitted by the frozen guest agent never contain
/// that separator.
pub fn parse_ls(listing: &str) -> Vec<LsEntry> {
    listing
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.rsplitn(4, '|');
            let mtime = parts.next()?.to_string();
            let is_dir = parts.next()? == "1";
            let size = parts.next()?.parse().ok()?;
            let name = parts.next()?.to_string();
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

    #[test]
    fn parse_ls_handles_lines_dirs_empty_and_pipe_names() {
        assert!(parse_ls("").is_empty());
        let entries = parse_ls(
            "a.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
             dir|0|1|2026-01-01T00:00:01.0000000Z\n\
             odd|name.txt|4|0|2026-01-01T00:00:02.0000000Z\n",
        );
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[0].size, 3);
        assert!(!entries[0].is_dir);
        assert_eq!(entries[1].name, "dir");
        assert!(entries[1].is_dir);
        assert_eq!(entries[2].name, "odd|name.txt");
    }

    #[test]
    fn fnv1a_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn host_guest_round_trip_does_not_ping_pong() {
        let mut sync = ShareSync::new(512);
        let host = HostFile {
            name: "x.txt".into(),
            size: 5,
            mtime_ms: 10,
        };
        assert_eq!(sync.on_host_scan(vec![host.clone()]), vec!["x.txt"]);
        let push = sync
            .on_host_file("x.txt".into(), b"hello".to_vec(), 10)
            .expect("host edit pushes guest");
        sync.on_put_ok("x.txt".into(), push.bytes.len() as u64, push.hash);

        let actions = sync.on_guest_listing(vec![LsEntry {
            name: "x.txt".into(),
            size: 5,
            is_dir: false,
            mtime: "guest-1".into(),
        }]);
        assert!(actions.is_empty(), "PUT landing mtime is only stamped");
        assert!(sync
            .on_guest_listing(vec![LsEntry {
                name: "x.txt".into(),
                size: 5,
                is_dir: false,
                mtime: "guest-1".into(),
            }])
            .is_empty());

        assert_eq!(
            sync.on_guest_listing(vec![LsEntry {
                name: "x.txt".into(),
                size: 6,
                is_dir: false,
                mtime: "guest-2".into(),
            }]),
            vec![SyncAction::Get {
                name: "x.txt".into()
            }]
        );
        match sync.on_guest_file("x.txt".into(), b"world!".to_vec(), None) {
            GuestFileOutcome::WriteHost(bytes) => assert_eq!(bytes, b"world!"),
            GuestFileOutcome::AlreadySynced => panic!("guest edit must write host"),
        }
        sync.note_host_stat("x.txt", 20);
        assert!(sync
            .on_host_file("x.txt".into(), b"world!".to_vec(), 20)
            .is_none());
    }

    #[test]
    fn guest_oversize_and_dir_skips_are_deduped_by_name_mtime_kind() {
        let mut sync = ShareSync::new(1);
        let entries = vec![
            LsEntry {
                name: "big.bin".into(),
                size: 2048,
                is_dir: false,
                mtime: "m1".into(),
            },
            LsEntry {
                name: "sub".into(),
                size: 0,
                is_dir: true,
                mtime: "d1".into(),
            },
        ];
        assert_eq!(sync.on_guest_listing(entries.clone()).len(), 2);
        assert!(sync.on_guest_listing(entries).is_empty());
        assert_eq!(
            sync.on_guest_listing(vec![LsEntry {
                name: "big.bin".into(),
                size: 2048,
                is_dir: false,
                mtime: "m2".into(),
            }]),
            vec![SyncAction::Skip {
                name: "big.bin".into(),
                reason: SkipReason::TooLarge { size: 2048 },
            }]
        );
    }
}
