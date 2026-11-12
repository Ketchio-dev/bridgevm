//! Resident share sync against a real host directory and a modelled NTFS guest:
//! case-only renames, refused or truncating PUTs and a vanished share root must
//! not delete, overwrite or rewrite data, nor stop guest-to-host sync.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::os::unix::fs::MetadataExt;
#[path = "share_alias_tests.rs"]
mod alias_tests;
#[path = "share_get_cap_tests.rs"]
mod get_cap_tests;
#[path = "share_get_reply_identity_tests.rs"]
mod get_reply_identity;
#[path = "share_guest_names_service_tests.rs"]
mod guest_names_tests;
#[path = "share_guest_destination_identity_tests.rs"]
mod guest_destination_identity;
#[path = "share_root_identity_tests.rs"]
mod root_identity;
#[path = "share_host_snapshot_tests.rs"]
mod host_snapshot;
#[path = "share_host_snapshot_limit_tests.rs"]
mod snapshot_limit;
#[path = "share_listing_report_tests.rs"]
mod listing_report_tests;
#[path = "share_put_unconfirmed_tests.rs"]
mod put_unconfirmed_tests;
#[path = "share_unseen_tests.rs"]
mod unseen_tests;
#[path = "share_excluded_tests.rs"]
mod excluded_tests;
#[path = "share_write_refusal_tests.rs"]
mod write_refusal_tests;
#[path = "share_adopt_tests.rs"]
mod adopt_tests;
#[path = "share_host_watch_service_tests.rs"]
mod host_watch_tests;
#[path = "share_listing_pause_tests.rs"]
mod listing_pause_tests;
#[path = "share_listing_scale_tests.rs"]
mod listing_scale_tests;
#[path = "share_listing_same_tests.rs"]
mod listing_same_tests;
#[path = "share_idle_tests.rs"]
mod idle_tests;

const ROOT: &str = "C:\\share";

/// Win32/NTFS as `bvagent.ps1` sees it under PowerShell 5.1: lookups ignore
/// case but keep the stored spelling, the last component loses trailing dots
/// and spaces, `<>:"|?*` and a bare device name such as `nul` are rejected,
/// code points are kept as given (no normalization), and a write truncates
/// before writing.
#[derive(Default)]
struct NtfsGuest {
    files: Vec<(String, Vec<u8>, u64)>,
    clock: u64,
    puts: Vec<String>,
    gets: Vec<String>,
    /// Names whose writes fail part-way (disk full) after truncation.
    failing: HashSet<String>,
    stream: Option<(String, usize, usize)>,
    /// PUTDATA chunks received.
    data_chunks: usize,
    /// Modification times the host sent (ms), as the agent sets them; listed in .NET's form.
    stamps: std::collections::HashMap<String, u128>,
    stream_time: Option<u128>,
    /// LSR fails, as for a folder the agent cannot read.
    ls_error: bool,
    /// The share folder is not there yet (a fresh guest): LSR fails until a RUN or PUT makes it.
    root_missing: bool,
    /// Runs as a GET is served (its name): a Mac edit made while the guest's copy is fetched.
    on_get: Option<Box<dyn FnMut(&str)>>,
    /// Rows listed after the files: the other entries of a large folder, written out once.
    bulk: String,
    /// As an agent with LSSAME (asked with a gen): its last listing and that listing's gen, the
    /// gens made, and the LSSAME replies sent.
    last_listing: Option<(String, String)>,
    gens: u64,
    same_replies: usize,
}

impl NtfsGuest {
    fn rel(path: &str) -> String {
        let rel = path.strip_prefix(ROOT).unwrap_or(path).trim_start_matches('\\');
        rel.trim_end_matches(['.', ' ']).to_string()
    }
    fn find(&self, rel: &str) -> Option<usize> {
        let key = rel.trim_end_matches(['.', ' ']).to_uppercase();
        self.files.iter().position(|(name, _, _)| name.to_uppercase() == key)
    }
    fn write(&mut self, rel: &str, bytes: &[u8]) {
        self.clock += 1;
        self.stamps.remove(&rel.to_uppercase());
        match self.find(rel) {
            Some(i) => (self.files[i].1, self.files[i].2) = (bytes.to_vec(), self.clock),
            None => self.files.push((rel.to_string(), bytes.to_vec(), self.clock)),
        }
    }
    fn append(&mut self, rel: &str, bytes: &[u8]) {
        let mut data = self.content(rel).unwrap_or_default().to_vec();
        data.extend_from_slice(bytes);
        self.write(rel, &data);
    }
    /// The time LSR lists: the one the host sent, else the write counter.
    fn listed_time(&self, name: &str, clock: u64) -> String {
        match self.stamps.get(&name.to_uppercase()) {
            Some(&ms) => dotnet_time(ms),
            None => format!("m{clock}"),
        }
    }
    fn content(&self, rel: &str) -> Option<&[u8]> {
        self.find(rel).map(|i| self.files[i].1.as_slice())
    }
    fn landed(&self, rel: &str, bytes: &[u8]) -> usize {
        if self.failing.contains(rel) { bytes.len() / 2 } else { bytes.len() }
    }
    fn reply(&mut self, line: &str) -> Vec<String> {
        let line = line.trim_end();
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let text = |b64: &str| String::from_utf8(base64_decode(b64).unwrap()).unwrap();
        let b64 = |bytes: &[u8]| {
            let mut out = String::new();
            base64_encode_into(bytes, &mut out);
            out
        };
        match verb {
            "PING" => vec!["PONG".into()],
            "LSR" if self.ls_error => vec!["ERR LSR Access to the path is denied.".into()],
            "LSR" if self.root_missing => {
                vec!["ERR LSR Cannot find path 'C:\\share\\' because it does not exist.".into()]
            }
            // The folder create after a failed first listing (share_listing_reply.rs).
            "RUN" => {
                self.root_missing = false;
                vec!["OUT 0 ".into()]
            }
            "LSR" => {
                let mut listing: String =
                    self.files.iter().map(|(n, b, m)| format!("{n}|{}|0|{}\n", b.len(), self.listed_time(n, *m))).collect();
                // As the agent: each folder is listed too.
                let mut folders = std::collections::BTreeSet::new();
                for (name, _, _) in &self.files {
                    for (i, _) in name.match_indices('\\') {
                        folders.insert(name[..i].to_string());
                    }
                }
                for folder in folders {
                    listing.push_str(&format!("{folder}|0|1|m0\n"));
                }
                listing.push_str(&self.bulk);
                let Some((_, gen)) = arg.split_once(' ') else {
                    return vec![format!("LSOK {}", b64(listing.as_bytes()))];
                };
                // As the agent: LSSAME while its last listing still holds, else that listing
                // with its gen.
                match self.last_listing.as_ref() {
                    Some((last, last_gen)) if *last == listing && last_gen == gen => {
                        self.same_replies += 1;
                        return vec![format!("LSSAME {gen}")];
                    }
                    Some((last, _)) if *last == listing => {}
                    _ => {
                        self.gens += 1;
                        self.last_listing = Some((listing, format!("{:032x}", self.gens)));
                    }
                }
                let (listing, gen) = self.last_listing.as_ref().unwrap();
                vec![format!("LSOK {} {gen}", b64(listing.as_bytes()))]
            }
            "GET" => {
                self.gets.push(Self::rel(&text(arg)));
                if let Some(hook) = self.on_get.as_mut() {
                    hook(&Self::rel(&text(arg)));
                }
                match self.find(&Self::rel(&text(arg))) {
                    Some(i) => vec![
                        format!("GETBEG {arg} {} 1", self.files[i].1.len()),
                        format!("GETCHUNK 0 {}", b64(&self.files[i].1)),
                        "GETEND 1".into(),
                    ],
                    None => vec!["ERR GET Could not find file".into()],
                }
            }
            "PUT" => {
                // As the agent: by position, ignoring the Mac time after the data.
                let mut parts = arg.split(' ');
                let (path, data) = (parts.next().unwrap(), parts.next().unwrap_or(""));
                let rel = Self::rel(&text(path));
                self.puts.push(rel.clone());
                self.root_missing = false;
                if rel.contains(['<', '>', ':', '"', '|', '?', '*']) {
                    return vec!["ERR PUT Illegal characters in path.".into()];
                }
                if ["CON", "PRN", "AUX", "NUL"].contains(&rel.rsplit('\\').next().unwrap().to_uppercase().as_str()) {
                    return vec!["ERR PUT FileStream was asked to open a device that was not a file.".into()];
                }
                let bytes = base64_decode(data).unwrap();
                self.write(&rel, &bytes[..self.landed(&rel, &bytes)]);
                if self.failing.contains(&rel) {
                    return vec!["ERR PUT There is not enough space on the disk.".into()];
                }
                if let Some(ms) = parts.next().and_then(|t| t.parse().ok()) {
                    self.stamps.insert(rel.to_uppercase(), ms);
                }
                vec![format!("PUTOK {path} {}", bytes.len())]
            }
            "PUTBEG" => {
                let mut parts = arg.split(' ');
                let rel = Self::rel(&text(parts.next().unwrap()));
                self.puts.push(rel.clone());
                self.root_missing = false;
                self.write(&rel, &[]);
                self.stream = Some((rel, parts.next().unwrap().parse().unwrap(), 0));
                self.stream_time = parts.nth(1).and_then(|t| t.parse().ok());
                vec!["OK PUTBEG".into()]
            }
            // As the agent: the header names the payload length, the payload is its own line.
            "PUTDATA" => {
                let (header, payload) = arg.split_once('\n').unwrap();
                let (seq, chars) = header.split_once(' ').unwrap();
                assert_eq!(payload.len(), chars.parse::<usize>().unwrap(), "PUTDATA {header}");
                self.data_chunks += 1;
                self.reply(&format!("PUTCHUNK {seq} {payload}"))
            }
            "PUTCHUNK" => {
                let (seq, data) = arg.split_once(' ').unwrap();
                let (rel, total, written) = self.stream.take().unwrap();
                let bytes = base64_decode(data).unwrap();
                let landed = self.landed(&rel, &bytes);
                self.append(&rel, &bytes[..landed]);
                self.stream = Some((rel, total, written + landed));
                vec![format!("OK PUTCHUNK {seq}")]
            }
            "PUTEND" => match self.stream.take().unwrap() {
                (_, total, written) if written != total => vec!["ERR PUTEND short".into()],
                (rel, _, written) => {
                    if let Some(ms) = self.stream_time.take() {
                        self.stamps.insert(rel.to_uppercase(), ms);
                    }
                    vec![format!("PUTOK {} {written}", b64(rel.as_bytes()))]
                }
            },
            "DEL" => match self.find(&Self::rel(&text(arg))) {
                Some(i) => {
                    self.files.remove(i);
                    vec!["OK DEL".into()]
                }
                None => vec!["OK DEL absent".into()],
            },
            other => panic!("unexpected guest request {other}"),
        }
    }
}

/// `LastWriteTimeUtc.ToString('o')` of a time in ms since 1970 (H. Hinnant's civil_from_days).
fn dotnet_time(ms: u128) -> String {
    let (secs, millis) = ((ms / 1000) as i64, ms % 1000);
    let (days, clock) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (clock / 3600, clock / 60 % 60, clock % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}0000Z")
}

struct Rig {
    h: AgentConsoleHarness,
    guest: NtfsGuest,
    now: Instant,
    dir: PathBuf,
}

impl Rig {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bvagent-winname-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.share = Some(ShareState::new(
            ShareSync::new(512),
            dir.clone(),
            ROOT.into(),
            Duration::from_millis(500),
        ));
        Self { h, guest: NtfsGuest::default(), now: Instant::now(), dir }
    }
    /// Service ticks with each request answered at once. Chunked PUT steps
    /// are written by the reply handler and forwarded like `service_pump`.
    fn run(&mut self, ticks: usize) {
        for _ in 0..ticks {
            // The state thread's mark writes land between ticks (unless a test holds them).
            if let Some(file) = self.h.share.as_ref().and_then(|share| share.sync_file.as_ref()) {
                file.wait_marks();
            }
            self.now += Duration::from_secs(1);
            self.h.service_enqueue(self.now);
            let Some(req) = self.h.queue.pop_front() else { continue };
            let Some(mut line) = self.h.service_req_line(&req, self.now).map(str::to_owned) else {
                continue;
            };
            self.h.last_send = Some(self.now);
            self.h.in_flight = Some((req, self.now));
            loop {
                self.h.service_line_scratch.clear();
                for reply in self.guest.reply(&line) {
                    self.h.handle_service_reply(&reply, None, None, self.now);
                }
                if self.h.in_flight.is_none() || self.h.service_line_scratch.is_empty() {
                    break;
                }
                line = self.h.service_line_scratch.clone();
            }
            assert!(self.h.in_flight.is_none(), "unanswered {line}");
        }
    }
    fn host(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.dir.join(name)).ok()
    }
    fn host_names(&self) -> Vec<String> {
        let entries = std::fs::read_dir(&self.dir).unwrap();
        let mut names: Vec<String> =
            entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
    fn inode(&self, name: &str) -> u64 {
        std::fs::metadata(self.dir.join(name)).unwrap().ino()
    }
    /// No further PUT, GET, delete or host change once both sides agree.
    fn assert_settled(&mut self) {
        let (puts, gets) = (self.guest.puts.len(), self.guest.gets.len());
        let (files, host) = (self.guest.files.len(), self.host_names());
        self.run(12);
        assert_eq!(self.guest.puts.len(), puts, "{:?}", self.guest.puts);
        assert_eq!(self.guest.gets.len(), gets, "{:?}", self.guest.gets);
        assert_eq!(self.guest.files.len(), files);
        assert_eq!(self.host_names(), host);
    }
    fn synced(label: &str, name: &str, bytes: &[u8]) -> Self {
        let mut rig = Self::new(label);
        std::fs::write(rig.dir.join(name), bytes).unwrap();
        rig.run(12);
        assert_eq!(rig.guest.content(name), Some(bytes));
        rig
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn same_size_guest_edit_before_first_post_put_listing_is_fetched() {
    let mut rig = Rig::new("post-put-edit");
    rig.h.note_agent_ready("GUEST v3-share2 putmtime", rig.now);
    std::fs::write(rig.dir.join("a.txt"), b"host").unwrap();
    for _ in 0..24 {
        rig.run(1);
        if !rig.guest.puts.is_empty() { break; }
    }
    assert_eq!(rig.guest.content("a.txt"), Some(&b"host"[..]));
    rig.guest.write("a.txt", b"edit");
    rig.run(24);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"edit"[..]));
    rig.assert_settled();
}

#[test]
fn dropped_fetch_guards_are_pruned_after_error_skip_and_restart() {
    let mut rig = Rig::new("fetch-guard-lifetime");
    let name = "doc.txt".to_string();
    rig.h.share.as_mut().unwrap().fetch_mac.insert(name.clone(), None);
    rig.h.queue.push_back(ServiceReq::ShareGet { name: name.clone() });
    rig.h.prune_fetch_guards();
    assert_eq!(rig.h.share.as_ref().unwrap().fetch_mac.len(), 1);
    rig.h.queue.clear(); // a skipped/dropped queued request
    rig.h.prune_fetch_guards();
    assert!(rig.h.share.as_ref().unwrap().fetch_mac.is_empty());
    rig.h.share.as_mut().unwrap().fetch_mac.insert(name.clone(), None);
    rig.h.in_flight = Some((ServiceReq::ShareGet { name: name.clone() }, rig.now));
    rig.h.handle_service_reply("ERR GET missing", None, None, rig.now);
    assert!(rig.h.share.as_ref().unwrap().fetch_mac.is_empty());
    rig.h.share.as_mut().unwrap().fetch_mac.insert(name.clone(), None);
    rig.h.in_flight = Some((ServiceReq::ShareGet { name }, rig.now));
    rig.h.resync_after_agent_restart("GUEST v3-share2", rig.now);
    assert!(rig.h.share.as_ref().unwrap().fetch_mac.is_empty());
    rig.h.share = None; // session end owns and drops the entire map
}

#[test]
fn refused_stale_guest_delete_fetches_the_edit_instead_of_retrying_delete() {
    let mut rig = Rig::synced("delete-guest-save", "doc.txt", b"original");
    rig.h.note_agent_ready("GUEST v3-share2 delcheck", rig.now);
    std::fs::remove_file(rig.dir.join("doc.txt")).unwrap();
    rig.guest.write("doc.txt", b"new guest save");
    let req = ServiceReq::ShareDel { name: "doc.txt".into(), direction: ShareDelDirection::HostToGuest };
    let wire = rig.h.service_req_line(&req, rig.now).unwrap().to_string();
    assert!(wire.contains(" 8 "), "expected size/time were not carried: {wire}");
    rig.h.in_flight = Some((req, rig.now));
    rig.h.handle_service_reply("OK DEL changed", None, None, rig.now);
    rig.run(16);
    assert_eq!(rig.host("doc.txt").as_deref(), Some(&b"new guest save"[..]));
    assert_eq!(rig.guest.content("doc.txt"), Some(&b"new guest save"[..]));
}

#[test]
fn unstamped_host_record_keeps_legacy_delete_behavior() {
    let mut rig = Rig::new("delete-unstamped-host");
    std::fs::write(rig.dir.join("doc.txt"), b"original").unwrap();
    let engine = &mut rig.h.share.as_mut().unwrap().engine;
    engine.on_guest_file("doc.txt".into(), b"original".to_vec(), None);
    // Successful publication without a readable destination mtime is valid.
    engine.on_host_write_succeeded("doc.txt", None);
    assert!(engine.delete_host_stat("doc.txt").is_none());
    assert!(engine.on_guest_listing_normalized(Vec::new()).iter().any(|a| matches!(a, SyncAction::DeleteHost { .. })));
    rig.h.handle_share_delete("doc.txt", ShareDelDirection::GuestToHost, rig.now);
    assert!(rig.host("doc.txt").is_none(), "unknown stamp must not silently suppress a planned delete");
}

#[test]
fn unstamped_guest_record_sends_legacy_delete_even_with_delcheck() {
    let mut rig = Rig::new("delete-unstamped-guest");
    rig.h.note_agent_ready("GUEST v3-share2 delcheck", rig.now);
    let engine = &mut rig.h.share.as_mut().unwrap().engine;
    // An acknowledged push has no guest stamp until its first matching listing.
    engine.on_put_ok("doc.txt".into(), 8, share_sync::fnv1a64(b"original"));
    assert!(engine.delete_guest_stat("doc.txt").is_none());
    assert!(engine.on_host_scan_normalized(Vec::new()).iter().any(|a| matches!(a, SyncAction::DeleteGuest { .. })));
    let req = ServiceReq::ShareDel { name: "doc.txt".into(), direction: ShareDelDirection::HostToGuest };
    let wire = rig.h.service_req_line(&req, rig.now).expect("unknown stamp must still send a delete").to_owned();
    assert_eq!(wire.split_whitespace().count(), 2, "fallback must be the legacy DEL path");
    rig.guest.write("doc.txt", b"original");
    assert_eq!(rig.guest.reply(&wire), ["OK DEL"]);
    rig.h.in_flight = Some((req, rig.now));
    rig.h.handle_service_reply("OK DEL", None, None, rig.now);
    assert!(rig.guest.content("doc.txt").is_none());
    assert!(!rig.h.share.as_ref().unwrap().engine.has_synced());
}

#[test]
fn queued_guest_delete_preserves_a_newer_mac_save() {
    let mut rig = Rig::synced("delete-mac-save", "doc.txt", b"original");
    // Guest disappearance was planned while the Mac still held the synced version.
    rig.guest.files.clear();
    let share = rig.h.share.as_mut().unwrap();
    assert!(share.engine.delete_host_stat("doc.txt").is_some());
    assert!(share.engine.on_guest_listing_normalized(Vec::new()).iter().any(|a| matches!(a, SyncAction::DeleteHost { .. })));
    std::fs::write(rig.dir.join("doc.txt"), b"newer Mac save").unwrap();
    rig.h.handle_share_delete("doc.txt", ShareDelDirection::GuestToHost, rig.now);
    assert_eq!(rig.host("doc.txt").as_deref(), Some(&b"newer Mac save"[..]));
    rig.run(16);
    assert_eq!(rig.guest.content("doc.txt"), Some(&b"newer Mac save"[..]));
}

#[test]
fn ordinary_fetch_preserves_mac_save_and_guest_conflict() {
    let mut rig = Rig::synced("fetch-mac-save", "doc.txt", b"original");
    std::fs::write(rig.dir.join("doc (Windows copy).txt"), b"existing copy").unwrap();
    rig.guest.write("doc.txt", b"guest edit");
    let path = rig.dir.join("doc.txt");
    rig.guest.on_get = Some(Box::new(move |_| { std::fs::write(&path, b"newer Mac save").unwrap(); }));
    rig.run(12);
    assert_eq!(rig.host("doc.txt").as_deref(), Some(&b"newer Mac save"[..]));
    assert_eq!(rig.host("doc (Windows copy).txt").as_deref(), Some(&b"existing copy"[..]));
    assert_eq!(rig.host("doc (Windows copy 2).txt").as_deref(), Some(&b"guest edit"[..]));
    rig.guest.on_get = None;
    rig.run(16);
    assert_eq!(rig.guest.content("doc (Windows copy 2).txt"), Some(&b"guest edit"[..]));
    assert!(!rig.guest.gets.iter().any(|n| n.contains("Windows copy")), "our pushed conflict must not be fetched back");
    rig.assert_settled();
    assert!(rig.h.share.as_ref().unwrap().fetch_mac.is_empty());
}

#[test]
fn host_case_only_rename_keeps_both_copies_and_never_rewrites_the_host() {
    let mut rig = Rig::synced("host-case", "report.txt", b"DATA");
    let gets = rig.guest.gets.len();
    std::fs::rename(rig.dir.join("report.txt"), rig.dir.join("Report.txt")).unwrap();
    rig.run(24);
    assert_eq!(rig.host_names(), ["Report.txt"]);
    assert_eq!(rig.host("Report.txt").as_deref(), Some(&b"DATA"[..]));
    assert_eq!(rig.guest.files[0].0, "report.txt");
    assert_eq!(rig.guest.content("report.txt"), Some(&b"DATA"[..]));
    // A later host edit reaches the guest's stored spelling without a GET
    // back, so the host file is never replaced (same inode).
    std::fs::write(rig.dir.join("Report.txt"), b"host edit").unwrap();
    let inode = rig.inode("Report.txt");
    rig.run(24);
    assert_eq!(rig.guest.content("report.txt"), Some(&b"host edit"[..]));
    assert_eq!(rig.guest.gets.len(), gets, "{:?}", rig.guest.gets);
    assert_eq!(rig.inode("Report.txt"), inode);
    assert_eq!(rig.host_names(), ["Report.txt"]);
    rig.guest.write("report.txt", b"guest edit");
    rig.run(24);
    assert_eq!(rig.host("Report.txt").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.host_names(), ["Report.txt"]);
    rig.assert_settled();
}

#[test]
fn guest_case_only_rename_keeps_both_copies_and_never_rewrites_the_host() {
    let mut rig = Rig::synced("guest-case", "report.txt", b"DATA");
    let gets = rig.guest.gets.len();
    rig.guest.files[0].0 = "Report.txt".into(); // NTFS rename keeps LastWriteTime
    rig.run(24);
    assert_eq!(rig.guest.gets.len(), gets, "{:?}", rig.guest.gets);
    assert_eq!(rig.host_names(), ["report.txt"]);
    assert_eq!(rig.host("report.txt").as_deref(), Some(&b"DATA"[..]));
    assert_eq!(rig.guest.content("Report.txt"), Some(&b"DATA"[..]));
    std::fs::write(rig.dir.join("report.txt"), b"host edit").unwrap();
    let inode = rig.inode("report.txt");
    rig.run(24);
    assert_eq!(rig.guest.content("Report.txt"), Some(&b"host edit"[..]));
    assert_eq!(rig.guest.gets.len(), gets, "{:?}", rig.guest.gets);
    assert_eq!(rig.inode("report.txt"), inode);
    rig.guest.write("Report.txt", b"guest edit");
    rig.run(24);
    assert_eq!(rig.host("report.txt").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.host_names(), ["report.txt"]);
    rig.assert_settled();
}

#[test]
fn lone_trailing_dot_name_is_never_deleted_or_duplicated() {
    let mut rig = Rig::synced("lone-dot", "foo.", b"BB");
    rig.run(24);
    assert_eq!(rig.host_names(), ["foo."]);
    assert_eq!(rig.guest.files[0].0, "foo");
    std::fs::write(rig.dir.join("foo."), b"host").unwrap();
    rig.run(24);
    assert_eq!(rig.guest.content("foo"), Some(&b"host"[..]));
    rig.guest.write("foo", b"guest");
    rig.run(24);
    assert_eq!(rig.host("foo.").as_deref(), Some(&b"guest"[..]));
    assert_eq!(rig.host_names(), ["foo."]);
    rig.assert_settled();
    // Deleting the guest's file is deleting this one file.
    rig.guest.files.clear();
    rig.run(24);
    assert!(rig.host_names().is_empty());
}

#[test]
fn rejected_put_waits_for_a_host_change_and_guest_listing_continues() {
    let mut rig = Rig::new("rejected");
    std::fs::write(rig.dir.join("nul"), b"q1").unwrap();
    rig.run(12);
    rig.guest.write("new.txt", b"from guest");
    rig.run(12);
    assert_eq!(rig.host("new.txt").as_deref(), Some(&b"from guest"[..]));
    let rejected = |rig: &Rig| rig.guest.puts.iter().filter(|p| p.as_str() == "nul").count();
    assert_eq!(rejected(&rig), 1, "{:?}", rig.guest.puts);
    std::fs::write(rig.dir.join("nul"), b"q2 changed").unwrap();
    rig.run(12);
    assert_eq!(rejected(&rig), 2, "{:?}", rig.guest.puts);
    rig.assert_settled();
}

#[test]
fn put_failing_after_truncating_the_guest_copy_never_overwrites_the_host() {
    // 100 B uses legacy PUT (ERR PUT); 30000 B uses PUTBEG..PUTEND (ERR PUTEND short).
    for (size, label) in [(100usize, "trunc-put"), (30_000, "trunc-putend")] {
        let mut rig = Rig::synced(label, "doc.bin", &vec![b'o'; size]);
        let newer = vec![b'n'; size + 7];
        std::fs::write(rig.dir.join("doc.bin"), &newer).unwrap();
        rig.guest.failing.insert("doc.bin".into());
        rig.run(24);
        let partial = rig.guest.content("doc.bin").unwrap().to_vec();
        assert!(partial.len() < newer.len() && partial.iter().all(|b| *b == b'n'), "{label}");
        assert_eq!(rig.host("doc.bin"), Some(newer.clone()), "{label}");
        assert!(rig.guest.gets.is_empty(), "{label}: {:?}", rig.guest.gets);
        assert_eq!(rig.guest.puts.len(), 2, "{label}: refused PUT is not retried");
        rig.guest.write("other.txt", b"guest->host still flows");
        rig.run(12);
        assert!(rig.host("other.txt").is_some(), "{label}");
        // The next host change retries and repairs the guest copy.
        rig.guest.failing.clear();
        let repaired = vec![b'r'; size + 9];
        std::fs::write(rig.dir.join("doc.bin"), &repaired).unwrap();
        rig.run(24);
        assert_eq!(rig.guest.content("doc.bin"), Some(&repaired[..]), "{label}");
        assert_eq!(rig.host("doc.bin"), Some(repaired), "{label}");
        rig.assert_settled();
    }
}

#[test]
fn refused_put_for_a_recordless_name_stored_in_another_case_never_overwrites_the_host() {
    // After a restart no records exist; the guest keeps an older spelling.
    let mut rig = Rig::new("refused-case");
    std::fs::write(rig.dir.join("Report.txt"), b"HOST ORIGINAL").unwrap();
    rig.guest.write("report.txt", b"older guest copy");
    rig.guest.failing.insert("Report.txt".into());
    rig.run(24);
    assert_eq!(rig.host("Report.txt").as_deref(), Some(&b"HOST ORIGINAL"[..]));
    assert!(rig.guest.gets.is_empty(), "{:?}", rig.guest.gets);
    assert_eq!(rig.host_names(), ["Report.txt"]);
}

#[test]
fn vanished_share_root_never_deletes_guest_copies() {
    let mut rig = Rig::synced("root-gone", "a.txt", b"A");
    let moved = rig.dir.with_extension("moved");
    let _ = std::fs::remove_dir_all(&moved);
    std::fs::rename(&rig.dir, &moved).unwrap();
    rig.run(12);
    std::fs::rename(&moved, &rig.dir).unwrap();
    assert_eq!(rig.guest.content("a.txt"), Some(&b"A"[..]));
    rig.run(12);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"A"[..]));
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"A"[..]));
}

#[test]
fn an_agent_that_takes_putdata_gets_large_files_in_mib_chunks_and_others_as_before() {
    for put_data in [false, true] {
        let mut rig = Rig::new(if put_data { "putdata" } else { "putchunk" });
        rig.h.share = Some(ShareState::new(ShareSync::new(4096), rig.dir.clone(), ROOT.into(), Duration::from_millis(500)));
        rig.h.put_data = put_data;
        let bytes: Vec<u8> = (0..2 * SHARE_PUT_DATA_CHUNK_BYTES + 7).map(|i| (i * 31 % 251) as u8).collect();
        std::fs::write(rig.dir.join("big.bin"), &bytes).unwrap();
        std::fs::write(rig.dir.join("small.txt"), b"one line").unwrap();
        rig.run(8);
        assert_eq!(rig.guest.content("big.bin"), Some(&bytes[..]));
        assert_eq!(rig.guest.content("small.txt"), Some(&b"one line"[..]));
        assert_eq!(rig.guest.data_chunks, if put_data { 3 } else { 0 });
    }
}

#[test]
fn one_host_scan_serves_a_run_of_puts_and_a_mac_edit_still_lands() {
    let mut rig = Rig::new("scan-once");
    rig.h.share.as_mut().unwrap().interval = Duration::from_secs(30);
    for i in 0..60 {
        std::fs::write(rig.dir.join(format!("f{i:02}.txt")), format!("file {i}")).unwrap();
    }
    // The first scan waits for the first guest listing (share_adopt.rs); the next one, a poll
    // interval later, sends all 60.
    rig.run(100);
    for i in 0..60 {
        assert_eq!(rig.guest.content(&format!("f{i:02}.txt")), Some(format!("file {i}").as_bytes()));
    }
    let scans = rig.h.share.as_ref().unwrap().host_scans;
    // One scan per poll interval (30 ticks here), not one per PUT (62 before).
    assert!(scans <= 6, "{scans} host scans for 60 PUTs");
    // An edit made during a run waits at most one poll interval.
    std::fs::write(rig.dir.join("f00.txt"), b"edited").unwrap();
    rig.run(40);
    assert_eq!(rig.guest.content("f00.txt"), Some(&b"edited"[..]));
}

#[test]
fn a_mac_file_restored_during_a_run_of_deletes_keeps_its_guest_copy() {
    let mut rig = Rig::new("scan-once-restore");
    rig.h.share.as_mut().unwrap().interval = Duration::from_secs(30);
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(rig.dir.join(name), name).unwrap();
    }
    rig.run(70);
    assert_eq!(rig.guest.files.len(), 3);
    let aside = rig.dir.with_extension("aside");
    std::fs::create_dir_all(&aside).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::rename(rig.dir.join(name), aside.join(name)).unwrap();
    }
    // The next scan, a poll interval on, queues the three deletions; one goes out per step.
    for _ in 0..40 {
        rig.run(1);
        if rig.guest.files.len() < 3 {
            break;
        }
    }
    assert_eq!(rig.guest.files.len(), 2);
    // Undo for a file whose deletion has not gone out yet, which Windows edited meanwhile.
    let kept = rig.guest.files.last().unwrap().0.clone();
    rig.guest.write(&kept, b"windows edit");
    std::fs::rename(aside.join(&kept), rig.dir.join(&kept)).unwrap();
    rig.run(70);
    assert_eq!(rig.guest.content(&kept), Some(&b"windows edit"[..]), "the restored file's guest copy was deleted");
    assert_eq!(rig.host(&kept).as_deref(), Some(&b"windows edit"[..]));
    let _ = std::fs::remove_dir_all(&aside);
}

#[test]
fn a_windows_twin_made_during_a_run_holds_a_cached_put() {
    let mut rig = Rig::new("scan-once-twin");
    rig.h.share.as_mut().unwrap().interval = Duration::from_secs(30);
    let names: Vec<String> = (0..6).map(|i| format!("f{i}")).collect();
    for name in &names {
        std::fs::write(rig.dir.join(name), b"ORIGINAL").unwrap();
    }
    rig.run(100);
    assert_eq!(rig.guest.files.len(), 6);
    rig.run(40); // past a poll interval: the next scan plans the run below
    let puts = rig.guest.puts.len();
    for name in &names {
        std::fs::write(rig.dir.join(name), b"MAC EDIT").unwrap();
    }
    for _ in 0..40 {
        rig.run(1);
        if rig.guest.puts.len() > puts {
            break;
        }
    }
    // A name whose PUT is still waiting gets a twin that is the same Windows name: both are held.
    let waiting = names.iter().find(|n| !rig.guest.puts[puts..].contains(n)).unwrap().clone();
    std::fs::write(rig.dir.join(format!("{waiting}.")), b"TWIN").unwrap();
    rig.run(40);
    assert!(!rig.guest.puts[puts..].contains(&waiting), "{waiting}: {:?}", rig.guest.puts);
    assert_eq!(rig.guest.content(&waiting), Some(&b"ORIGINAL"[..]));
}

#[test]
fn a_mac_folder_restored_during_a_run_of_deletes_keeps_its_guest_copies() {
    let mut rig = Rig::new("scan-once-folder");
    rig.h.share.as_mut().unwrap().interval = Duration::from_secs(30);
    std::fs::create_dir_all(rig.dir.join("x")).unwrap();
    for name in ["a", "b", "c", "d"] {
        std::fs::write(rig.dir.join("x").join(name), name).unwrap();
    }
    rig.run(70);
    assert_eq!(rig.guest.files.len(), 4);
    let aside = rig.dir.with_extension("aside-folder");
    std::fs::rename(rig.dir.join("x"), &aside).unwrap();
    for _ in 0..40 {
        rig.run(1);
        if rig.guest.files.len() < 4 {
            break;
        }
    }
    assert_eq!(rig.guest.files.len(), 3);
    // Undo: the folder comes back, and Windows had edited a file still waiting for its deletion.
    let kept = rig.guest.files.last().unwrap().0.clone();
    rig.guest.write(&kept, b"windows edit");
    std::fs::rename(&aside, rig.dir.join("x")).unwrap();
    rig.run(70);
    assert_eq!(rig.guest.files.len(), 4, "{:?}", rig.guest.files.iter().map(|f| &f.0).collect::<Vec<_>>());
    assert_eq!(rig.guest.content(&kept), Some(&b"windows edit"[..]), "a restored file's guest copy was deleted");
}

#[test]
fn a_new_folder_beside_a_cached_puts_folder_replans_the_run() {
    let mut rig = Rig::new("scan-once-ancestor");
    rig.h.share.as_mut().unwrap().interval = Duration::from_secs(30);
    std::fs::create_dir_all(rig.dir.join("x")).unwrap();
    let names: Vec<String> = (0..6).map(|i| format!("x/f{i}")).collect();
    for name in &names {
        std::fs::write(rig.dir.join(name), b"ORIGINAL").unwrap();
    }
    rig.run(100);
    rig.run(40);
    let scans = rig.h.share.as_ref().unwrap().host_scans;
    for name in &names {
        std::fs::write(rig.dir.join(name), b"MAC EDIT").unwrap();
    }
    for _ in 0..40 {
        rig.run(1);
        if rig.h.share.as_ref().unwrap().host_scans > scans {
            break;
        }
    }
    rig.run(1);
    // A folder made at the root, a level above the cached PUTs' folder, also replans.
    let before = rig.h.share.as_ref().unwrap().host_scans;
    std::fs::create_dir_all(rig.dir.join("y")).unwrap();
    rig.run(1);
    assert_eq!(rig.h.share.as_ref().unwrap().host_scans, before + 1);
}
