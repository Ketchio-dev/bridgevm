//! Windows print jobs through the resident service loop, against a guest that answers GETRANGE,
//! DEL and PRINTWATCH the way bvagent.ps1 does.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::collections::HashMap;

const MIB: u64 = 1 << 20;
const READY: &str = "GUEST v3-share2 putdata bigfile lssame startapps print agent=0123456789abcdef";

struct Guest {
    files: HashMap<String, Vec<u8>>,
    /// GETRANGE sends at most this many GETCHUNKs (a reply cut short).
    chunks_cap: Option<usize>,
    /// GETRANGE answers ERR.
    error: bool,
    /// The file's time GETRANGE reports, and the one after the first range (the file changed).
    mtime: String,
    mtime_after_first: Option<String>,
    deleted: Vec<String>,
}

impl Guest {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
            chunks_cap: None,
            error: false,
            mtime: "2026-11-11T00:00:00.0000000Z".to_owned(),
            mtime_after_first: None,
            deleted: Vec::new(),
        }
    }

    fn answer(&mut self, line: &str) -> Vec<String> {
        let line = line.trim_end();
        let (verb, arg) = line.split_once(' ').unwrap_or((line, ""));
        let reply = match verb {
            "PRINTWATCH" => format!("OUT 0 {}", base64_encode(b"BVPRINT printer=ready")),
            "GETRANGE" if self.error => "ERR GETRANGE The process cannot access the file".to_owned(),
            "GETRANGE" => {
                let mut fields = arg.split(' ');
                let path_b64 = fields.next().unwrap();
                let path = String::from_utf8(base64_decode(path_b64).unwrap()).unwrap();
                let (offset, want): (u64, u64) = (fields.next().unwrap().parse().unwrap(), fields.next().unwrap().parse().unwrap());
                let Some(bytes) = self.files.get(&path) else {
                    return vec!["ERR GETRANGE Could not find file".to_owned()];
                };
                let size = bytes.len() as u64;
                let len = want.min(16 << 20).min(size.saturating_sub(offset));
                let range = &bytes[offset as usize..(offset + len) as usize];
                let chunks: Vec<&[u8]> = range.chunks(768 * 1024).collect();
                let mut lines = vec![format!("GETBEG {path_b64} {len} {} {offset} {size} {}", chunks.len(), self.mtime)];
                for (seq, chunk) in chunks.iter().enumerate().take(self.chunks_cap.unwrap_or(usize::MAX)) {
                    lines.push(format!("GETCHUNK {seq} {}", base64_encode(chunk)));
                }
                lines.push(format!("GETEND {}", chunks.len()));
                if let Some(mtime) = self.mtime_after_first.take() {
                    self.mtime = mtime;
                }
                return lines;
            }
            "DEL" => {
                let path = String::from_utf8(base64_decode(arg).unwrap()).unwrap();
                self.deleted.push(path.clone());
                if self.files.remove(&path).is_some() { "OK DEL" } else { "OK DEL absent" }.to_owned()
            }
            _ => format!("OUT 255 {}", base64_encode(format!("unknown token: {verb}").as_bytes())),
        };
        vec![reply]
    }
}

struct Rig {
    h: AgentConsoleHarness,
    guest: Guest,
    /// Each request's first line, verb and offset only.
    wire: Vec<String>,
    now: Instant,
    dir: PathBuf,
}

/// A PDF of `len` bytes, each byte its own.
fn pdf(len: usize, seed: u8) -> Vec<u8> {
    let mut bytes: Vec<u8> = (0..len).map(|at| (at as u8).wrapping_mul(31) ^ (at >> 11) as u8 ^ seed).collect();
    let head = b"%PDF-1.7\n";
    let n = head.len().min(len);
    bytes[..n].copy_from_slice(&head[..n]);
    bytes
}

fn take_print_lines() -> Vec<String> {
    PRINT_LINES.with(|lines| std::mem::take(&mut *lines.borrow_mut()))
}

fn guest_path(n: u64) -> String {
    format!(r"C:\Users\u\AppData\Local\BridgeVM\Print\{:019}.pdf", 638_982_000_000_000_000 + n)
}

/// Every file name in `dir`.
fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

impl Rig {
    /// A resident service whose agent has `print`, with the app's PRINTWATCH sent when `watch`.
    fn new(name: &str, watch: bool) -> Self {
        let mut h = harness();
        h.service = true;
        h.commands.clear();
        h.clock.boot_resync_queued = true;
        let now = Instant::now();
        assert!(h.next_command_line(0, now).is_none());
        let dir = std::env::temp_dir().join(format!("bridgevm-print-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        h.print = PrintJobs::new(Some(dir.clone()));
        h.note_agent_ready(READY, now);
        let mut rig = Self { h, guest: Guest::new(), wire: Vec::new(), now, dir };
        if watch {
            rig.h.queue.push_back(ServiceReq::Ctl(PRINT_WATCH_VERB.to_owned()));
            rig.settle();
            assert_eq!(rig.wire, ["PRINTWATCH"]);
            rig.wire.clear();
        }
        take_print_lines();
        rig
    }

    /// Every request in lockstep with the guest's replies until the wire is quiet.
    fn settle(&mut self) {
        loop {
            let mut sent = Vec::new();
            self.h.service_pump_with_sender(self.now, &mut |bytes| sent.push(bytes.to_vec()));
            if sent.is_empty() {
                break;
            }
            for bytes in sent {
                let line = String::from_utf8(bytes).unwrap();
                let mut fields = line.trim_end().split(' ');
                let verb = fields.next().unwrap().to_owned();
                self.wire.push(if verb == "GETRANGE" { format!("GETRANGE {}", fields.nth(1).unwrap()) } else { verb });
                for reply in self.guest.answer(&line) {
                    self.h.handle_service_reply_with_sender(&reply, self.now, &mut |_| panic!("no reply sends"));
                    // Never a whole range in memory: nothing accumulates as a GET would.
                    assert!(self.h.get_accum.is_none());
                }
            }
        }
        assert!(self.h.in_flight.is_none());
    }

    /// The guest holds `bytes` at `path` and the agent announces it between replies.
    fn job(&mut self, path: &str, bytes: Vec<u8>) {
        let len = bytes.len();
        self.guest.files.insert(path.to_owned(), bytes);
        self.announce(&format!("{} {len}", base64_encode(path.as_bytes())));
    }

    fn announce(&mut self, rest: &str) {
        self.h.handle_service_reply_with_sender(&format!("PRINTJOB {rest}"), self.now, &mut |_| panic!("no reply sends"));
    }

    fn take_wire(&mut self) -> Vec<String> {
        std::mem::take(&mut self.wire)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The published job a `BVPRINT job` line names: its bytes, SHA-256 and Mac path.
fn published(line: &str) -> (u64, String, PathBuf) {
    let rest = line.strip_prefix("BVPRINT job bytes=").unwrap_or_else(|| panic!("{line}"));
    let (bytes, rest) = rest.split_once(" sha256=").unwrap();
    let (sha, path) = rest.split_once(" path=").unwrap();
    (bytes.parse().unwrap(), sha.to_owned(), PathBuf::from(path))
}

#[test]
fn a_job_comes_over_in_ranges_hashed_as_it_comes_and_ends_in_one_line_then_the_guest_copy_goes() {
    let mut rig = Rig::new("whole", true);
    let bytes = pdf((20 * MIB + 5) as usize, 1);
    let path = guest_path(1);
    rig.job(&path, bytes.clone());
    rig.settle();
    let ranges = [0, 8 * MIB, 16 * MIB].map(|offset| format!("GETRANGE {offset}"));
    assert_eq!(rig.take_wire(), [&ranges[..], &["DEL".to_owned()]].concat());
    let lines = take_print_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let (len, sha, mac) = published(&lines[0]);
    assert_eq!(len, bytes.len() as u64);
    assert_eq!(sha, hex_lower(&Sha256::digest(&bytes)));
    assert_eq!(mac.parent(), Some(rig.dir.as_path()), "the probe's own folder");
    let name = mac.file_name().unwrap().to_str().unwrap();
    assert!(name.strip_suffix(".pdf").unwrap().bytes().all(|b| b.is_ascii_digit()), "named by the probe: {name}");
    assert!(std::fs::read(&mac).unwrap() == bytes);
    assert_eq!(files_in(&rig.dir), [name], "no temporary left");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&rig.dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&mac).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(rig.guest.deleted, [path]);

    // The next job gets the next name, and two jobs announced together come one after the other;
    // a Korean user name is a path like any other.
    rig.job(&guest_path(2), pdf(1000, 2));
    rig.job(&guest_path(3).replace(r"\u\", "\\\u{d64d}\u{ae38}\u{b3d9}\\"), pdf(2000, 3));
    rig.settle();
    assert_eq!(rig.take_wire(), ["GETRANGE 0", "DEL", "GETRANGE 0", "DEL"]);
    let lines = take_print_lines();
    let sizes: Vec<u64> = lines.iter().map(|line| published(line).0).collect();
    assert_eq!(sizes, [1000, 2000]);
    assert_eq!(files_in(&rig.dir).len(), 3);
}

#[test]
fn jobs_wait_for_the_apps_printwatch_a_print_folder_and_an_agent_that_has_it() {
    // The app never asked: the guest's announcement is ignored, and its file stays there.
    let mut rig = Rig::new("unasked", false);
    rig.job(&guest_path(1), pdf(1000, 0));
    rig.settle();
    assert!(rig.take_wire().is_empty() && take_print_lines().is_empty());
    // Asked, then turned off.
    rig.h.queue.push_back(ServiceReq::Ctl(format!("{PRINT_WATCH_VERB} off")));
    rig.settle();
    assert_eq!(rig.take_wire(), ["PRINTWATCH"]);
    rig.job(&guest_path(2), pdf(1000, 0));
    rig.settle();
    assert!(rig.take_wire().is_empty() && take_print_lines().is_empty());

    // A probe without a print folder takes nothing either.
    let mut rig = Rig::new("nofolder", true);
    rig.h.print = PrintJobs::new(None);
    rig.h.print.watch = Some(true);
    rig.job(&guest_path(1), pdf(1000, 0));
    rig.settle();
    assert!(rig.take_wire().is_empty() && take_print_lines().is_empty());

    // An agent without `print` is not asked; the update's agent is, and so is a restarted one.
    let mut rig = Rig::new("oldagent", false);
    rig.h.note_agent_ready("GUEST v3-share2 putdata bigfile lssame", rig.now);
    rig.h.queue.push_back(ServiceReq::Ctl(PRINT_WATCH_VERB.to_owned()));
    rig.settle();
    assert!(rig.take_wire().is_empty(), "held for an agent that has it");
    rig.h.handle_service_reply_with_sender(&format!("READY {READY}"), rig.now, &mut |_| {});
    rig.settle();
    assert!(rig.take_wire().contains(&"PRINTWATCH".to_owned()));
    rig.h.handle_service_reply_with_sender(&format!("READY {READY}"), rig.now, &mut |_| {});
    rig.settle();
    assert!(rig.take_wire().contains(&"PRINTWATCH".to_owned()), "a restarted agent is asked again");
    rig.h.handle_service_reply_with_sender("READY GUEST v3-share2 putdata bigfile", rig.now, &mut |_| {});
    rig.settle();
    assert!(!rig.take_wire().contains(&"PRINTWATCH".to_owned()), "not an agent without it");
    rig.h.handle_service_reply_with_sender(&format!("READY {READY}"), rig.now, &mut |_| {});
    rig.settle();
    rig.take_wire();
    rig.job(&guest_path(1), pdf(1000, 0));
    rig.settle();
    assert_eq!(rig.take_wire(), ["GETRANGE 0", "DEL"]);
    assert_eq!(take_print_lines().len(), 1);
}

#[test]
fn guest_announcements_are_bounded_and_each_refusal_is_one_line() {
    let mut rig = Rig::new("bounds", true);
    let b64 = |path: &str| base64_encode(path.as_bytes());
    for (rest, reason) in [
        ("x".to_owned(), "malformed"),
        ("!!! 5".to_owned(), "malformed"),
        (format!("{} -1", b64(&guest_path(1))), "malformed"),
        (format!("{} 5 6", b64(&guest_path(1))), "malformed"),
        (format!("{} 5", base64_encode(&[0xff, 0xfe, b'.', b'p', b'd', b'f'])), "path"),
        (format!("{} 5", b64(r"C:\x\job.exe")), "path"),
        // Ends in multibyte characters: never sliced inside one.
        (format!("{} 5", b64("C:\\x\\\u{d55c}\u{ae00}")), "path"),
        (format!("{} 5", b64("C:\\x\\\u{1b}[2J.pdf")), "path"),
        (format!("{} 5", b64(&format!(r"C:\{}.pdf", "a".repeat(1100)))), "path"),
    ] {
        rig.announce(&rest);
        assert_eq!(take_print_lines(), [format!("BVPRINT refused reason={reason} bytes=0")], "{rest}");
    }
    rig.settle();
    assert!(rig.take_wire().is_empty(), "nothing asked of a path that is not one");

    // Too large or empty: one line, and the guest copy goes; nothing is fetched.
    let big = MAX_PRINT_JOB_BYTES + 1;
    rig.announce(&format!("{} {big}", b64(&guest_path(1))));
    rig.announce(&format!("{} 0", b64(&guest_path(2))));
    assert_eq!(
        take_print_lines(),
        [format!("BVPRINT refused reason=too-large bytes={big}"), "BVPRINT refused reason=empty bytes=0".to_owned()]
    );
    rig.settle();
    assert_eq!(rig.take_wire(), ["DEL", "DEL"]);
    // The limit itself is taken.
    rig.announce(&format!("{} {MAX_PRINT_JOB_BYTES}", b64(&guest_path(3))));
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| sent.push(String::from_utf8(bytes.to_vec()).unwrap()));
    assert_eq!(sent, [format!("GETRANGE {} 0 {}\n", b64(&guest_path(3)), 8 * MIB)]);
    rig.h.in_flight = None;
    rig.h.print_agent_restarted();

    // At most MAX_PRINT_JOBS at once; the next is refused, and one already waiting is not taken twice.
    for n in 10..10 + MAX_PRINT_JOBS as u64 {
        rig.job(&guest_path(n), pdf(100 + n as usize, n as u8));
    }
    rig.job(&guest_path(10), pdf(110, 10));
    assert!(take_print_lines().is_empty());
    rig.job(&guest_path(99), pdf(500, 9));
    assert_eq!(take_print_lines(), ["BVPRINT refused reason=too-many bytes=500"]);
    rig.settle();
    let lines = take_print_lines();
    assert_eq!(lines.len(), MAX_PRINT_JOBS, "{lines:?}");
    assert!(lines.iter().all(|line| line.starts_with("BVPRINT job ")));
    assert_eq!(rig.guest.deleted.iter().filter(|path| **path == guest_path(99)).count(), 1);
}

#[test]
fn a_partial_or_changed_job_is_never_published_and_ends_in_one_line() {
    type Spoil = fn(&mut Guest);
    // (the line's reason, what the guest does, its file, the size the agent announced if not the file's)
    let cases: [(&str, Spoil, Vec<u8>, Option<u64>); 5] = [
        ("changed", |_| {}, pdf(2000, 0), Some(3000)),
        ("short", |guest| guest.chunks_cap = Some(1), pdf(2 * MIB as usize, 0), None),
        ("get-error", |guest| guest.error = true, pdf(1000, 0), None),
        ("not-pdf", |_| {}, b"PK\x03\x04 a zip, not a PDF".to_vec(), None),
        ("changed", |guest| guest.mtime_after_first = Some("2026-11-11T00:00:09.0000000Z".to_owned()), pdf(9 * MIB as usize, 0), None),
    ];
    for (n, (reason, spoil, bytes, announced)) in cases.into_iter().enumerate() {
        let announced = announced.unwrap_or(bytes.len() as u64);
        let mut rig = Rig::new(&format!("partial-{n}"), true);
        spoil(&mut rig.guest);
        let path = guest_path(n as u64);
        rig.guest.files.insert(path.clone(), bytes);
        rig.announce(&format!("{} {announced}", base64_encode(path.as_bytes())));
        rig.settle();
        assert_eq!(take_print_lines(), [format!("BVPRINT refused reason={reason} bytes={announced}")], "case {n}");
        assert!(files_in(&rig.dir).is_empty(), "case {n}: nothing published, no temporary: {:?}", files_in(&rig.dir));
        assert_eq!(rig.guest.deleted, [path], "case {n}");
        // The service goes on: the next job comes over.
        rig.guest = Guest::new();
        rig.job(&guest_path(50), pdf(1000, 5));
        rig.settle();
        assert_eq!(take_print_lines().len(), 1, "case {n}");
    }
}

#[test]
fn an_agent_restart_or_a_stall_mid_job_publishes_nothing() {
    // A restart after the first range: no line and no file; the new agent announces the job again.
    let mut rig = Rig::new("restart", true);
    let path = guest_path(1);
    let bytes = pdf((12 * MIB) as usize, 7);
    rig.job(&path, bytes.clone());
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |b| sent.push(String::from_utf8(b.to_vec()).unwrap()));
    for reply in rig.guest.answer(&sent[0]) {
        rig.h.handle_service_reply_with_sender(&reply, rig.now, &mut |_| {});
    }
    assert_eq!(files_in(&rig.dir).len(), 1, "the first range written to its temporary");
    rig.h.handle_service_reply_with_sender(&format!("READY {READY}"), rig.now, &mut |_| {});
    rig.settle();
    assert_eq!(rig.take_wire(), ["PRINTWATCH"], "nothing more of the old agent's job");
    assert!(take_print_lines().is_empty() && files_in(&rig.dir).is_empty());
    rig.job(&path, bytes.clone());
    rig.settle();
    let lines = take_print_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(published(&lines[0]).1, hex_lower(&Sha256::digest(&bytes)));

    // No reply to a range for three overdue beats: the job ends, its temporary goes.
    let mut rig = Rig::new("stall", true);
    rig.job(&guest_path(2), pdf(1000, 1));
    rig.h.service_pump_with_sender(rig.now, &mut |_| {});
    assert!(matches!(rig.h.in_flight, Some((ServiceReq::PrintRange, _))));
    for _ in 0..3 {
        rig.now += SERVICE_OVERDUE_INTERVAL;
        rig.h.service_pump_with_sender(rig.now, &mut |_| {});
        rig.h.in_flight.take_if(|(req, _)| matches!(req, ServiceReq::PrintDel { .. }));
    }
    assert_eq!(take_print_lines(), ["BVPRINT refused reason=stalled bytes=1000"]);
    assert!(files_in(&rig.dir).is_empty());
}

#[test]
fn a_job_announced_again_after_its_delete_was_lost_is_deleted_not_printed_twice() {
    let mut rig = Rig::new("again", true);
    let path = guest_path(1);
    rig.job(&path, pdf(1000, 1));
    rig.settle();
    assert_eq!(take_print_lines().len(), 1);
    rig.take_wire();
    rig.job(&path, pdf(1000, 1));
    rig.settle();
    assert_eq!(rig.take_wire(), ["DEL"]);
    assert!(take_print_lines().is_empty());
}

#[test]
fn the_first_harness_clears_only_files_named_as_jobs() {
    let dir = std::env::temp_dir().join(format!("bridgevm-print-clear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("12.pdf.d")).unwrap();
    for name in ["1.pdf", "22.pdf.part", "notes.txt", "x1.pdf", ".pdf", "3.PDF"] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    clear_jobs(&dir);
    assert_eq!(files_in(&dir), [".pdf", "12.pdf.d", "3.PDF", "notes.txt", "x1.pdf"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn printwatch_goes_raw_and_the_ready_marker_says_who_has_it() {
    assert!(is_raw_verb(PRINT_WATCH_VERB));
    assert_eq!(command_wire_line("PRINTWATCH"), "PRINTWATCH\n");
    assert_eq!(command_wire_line("PRINTWATCH off"), "PRINTWATCH off\n");
    assert_eq!(print_watch_command("PRINTWATCH"), Some(true));
    assert_eq!(print_watch_command("PRINTWATCH off"), Some(false));
    assert_eq!(print_watch_command("PRINTWATCHER"), None);
    assert!(ready_has_print(READY));
    assert!(!ready_has_print("GUEST v3-share2 putdata bigfile lssame startapps"));
    assert!(!ready_has_print("GUEST printer"));
}
