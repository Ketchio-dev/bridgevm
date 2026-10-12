//! Resident share sync against a real host directory and a modelled NTFS guest:
//! case-only renames, refused or truncating PUTs and a vanished share root must
//! not delete, overwrite or rewrite data, nor stop guest-to-host sync.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::os::unix::fs::MetadataExt;
#[path = "share_alias_tests.rs"]
mod alias_tests;
#[path = "share_unseen_tests.rs"]
mod unseen_tests;

const ROOT: &str = "C:\\share";

/// Win32/NTFS as `bvagent.ps1` sees it under PowerShell 5.1: lookups ignore
/// case but keep the stored spelling, the last component loses trailing dots
/// and spaces, `<>:"|?*` are rejected, and a write truncates before writing.
#[derive(Default)]
struct NtfsGuest {
    files: Vec<(String, Vec<u8>, u64)>,
    clock: u64,
    puts: Vec<String>,
    gets: Vec<String>,
    /// Names whose writes fail part-way (disk full) after truncation.
    failing: HashSet<String>,
    stream: Option<(String, usize, usize)>,
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
            "LSR" => {
                let listing: String =
                    self.files.iter().map(|(n, b, m)| format!("{n}|{}|0|m{m}\n", b.len())).collect();
                vec![format!("LSOK {}", b64(listing.as_bytes()))]
            }
            "GET" => {
                self.gets.push(Self::rel(&text(arg)));
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
                let (path, data) = arg.split_once(' ').unwrap();
                let rel = Self::rel(&text(path));
                self.puts.push(rel.clone());
                if rel.contains(['<', '>', ':', '"', '|', '?', '*']) {
                    return vec!["ERR PUT Illegal characters in path.".into()];
                }
                let bytes = base64_decode(data).unwrap();
                self.write(&rel, &bytes[..self.landed(&rel, &bytes)]);
                if self.failing.contains(&rel) {
                    return vec!["ERR PUT There is not enough space on the disk.".into()];
                }
                vec![format!("PUTOK {path} {}", bytes.len())]
            }
            "PUTBEG" => {
                let mut parts = arg.split(' ');
                let rel = Self::rel(&text(parts.next().unwrap()));
                self.puts.push(rel.clone());
                self.write(&rel, &[]);
                self.stream = Some((rel, parts.next().unwrap().parse().unwrap(), 0));
                vec!["OK PUTBEG".into()]
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
                (rel, _, written) => vec![format!("PUTOK {} {written}", b64(rel.as_bytes()))],
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
        h.share = Some(ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: ROOT.into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        });
        Self { h, guest: NtfsGuest::default(), now: Instant::now(), dir }
    }
    /// Service ticks with each request answered at once. Chunked PUT steps
    /// are written by the reply handler and forwarded like `service_pump`.
    fn run(&mut self, ticks: usize) {
        for _ in 0..ticks {
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
    std::fs::write(rig.dir.join("Q?.txt"), b"q1").unwrap();
    rig.run(12);
    rig.guest.write("new.txt", b"from guest");
    rig.run(12);
    assert_eq!(rig.host("new.txt").as_deref(), Some(&b"from guest"[..]));
    let rejected = |rig: &Rig| rig.guest.puts.iter().filter(|p| p.as_str() == "Q?.txt").count();
    assert_eq!(rejected(&rig), 1, "{:?}", rig.guest.puts);
    std::fs::write(rig.dir.join("Q?.txt"), b"q2 changed").unwrap();
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
