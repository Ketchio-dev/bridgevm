//! Windows print jobs for the Mac (the app's "Mac으로 인쇄", HvfGuestPrint.swift).
//!
//! The app sends `PRINTWATCH` at each agent start; the agent makes the printer `Mac (BridgeVM)` and
//! then writes `PRINTJOB <b64(utf8 path)> <bytes>` between replies for each finished job
//! (bvagent.ps1, print). Those lines are guest data. They are taken only after the app's PRINTWATCH
//! went through this harness, bounded in size and number, and their path only ever goes back to
//! the guest (GETRANGE, DEL): the Mac file is named by this probe's start and its own counter. A job
//! comes over in GETRANGE ranges, each chunk written to `<dir>/<start>-<n>.pdf.part` and hashed as it
//! comes, so memory holds one chunk; only a whole job that begins with `%PDF-` becomes
//! `<dir>/<start>-<n>.pdf`, which the app deletes once its print panel is over. Each job ends
//! in one line the app reads from run.log, then the guest file is deleted:
//! `BVPRINT job bytes=<n> sha256=<hex> path=<mac path>` (the path last: it may hold spaces), or
//! `BVPRINT refused reason=<r> bytes=<n>`. An agent restart abandons the job under way without a
//! line: the new agent announces it again.

use super::*;
use sha2::{Digest, Sha256};
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::sync::atomic::{AtomicBool, AtomicU64};

#[cfg(test)]
#[path = "print_jobs_tests.rs"]
mod tests;

/// `<evidence>/print`: where finished jobs go. Unset, this probe takes no print jobs.
pub(super) const PRINT_DIR_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_PRINT_DIR";
/// The app's request: `PRINTWATCH` on, `PRINTWATCH off` off.
pub(super) const PRINT_WATCH_VERB: &str = "PRINTWATCH";
/// The largest job taken.
pub(super) const MAX_PRINT_JOB_BYTES: u64 = 256 << 20;
/// Jobs waiting or coming over at once; the agent keeps at most as many.
pub(super) const MAX_PRINT_JOBS: usize = 8;
/// The longest guest path taken, in UTF-8 bytes.
const MAX_PRINT_PATH_BYTES: usize = 1024;
/// Guest paths of jobs that ended here, kept so that one announced again is deleted, not printed twice.
const DONE_KEPT: usize = 32;

/// The Mac files' names: this process's start (Unix ms) and a counter (harnesses are re-created per
/// boot), so a job an earlier probe published, still waiting for its print panel, keeps its name.
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);
static GENERATION: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
static CLEARED: AtomicBool = AtomicBool::new(false);

fn print_generation() -> u64 {
    *GENERATION.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64)
    })
}

#[cfg(test)]
thread_local! {
    pub(super) static PRINT_LINES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn print_line(line: String) {
    #[cfg(test)]
    PRINT_LINES.with(|lines| lines.borrow_mut().push(line.clone()));
    println!("{line}");
}

fn refused_line(reason: &str, bytes: u64) -> String {
    format!("BVPRINT refused reason={reason} bytes={bytes}")
}

/// Whether the READY agent has PRINTWATCH and PRINTJOB.
pub(super) fn ready_has_print(rest: &str) -> bool {
    rest.split(' ').any(|token| token == "print")
}

/// `PRINTWATCH` is on, `PRINTWATCH off` off; None for any other command.
pub(super) fn print_watch_command(command: &str) -> Option<bool> {
    match command.strip_prefix(PRINT_WATCH_VERB)? {
        "" => Some(true),
        " off" => Some(false),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GuestJob {
    pub(super) path: String,
    pub(super) bytes: u64,
}

/// A PRINTJOB line's `<b64(utf8 path)> <bytes>`, or why it is refused.
pub(super) fn parse_print_job(rest: &str) -> Result<GuestJob, &'static str> {
    let (b64, bytes) = rest.split_once(' ').ok_or("malformed")?;
    let bytes: u64 = bytes.parse().map_err(|_| "malformed")?;
    if b64.len() > MAX_PRINT_PATH_BYTES.div_ceil(3) * 4 {
        return Err("path");
    }
    let path = String::from_utf8(base64_decode(b64).map_err(|_| "malformed")?).map_err(|_| "path")?;
    // Bytes, not a str slice: the guest's last characters may be several bytes long.
    let pdf = path.len() > 4 && path.as_bytes()[path.len() - 4..].eq_ignore_ascii_case(b".pdf");
    if path.len() > MAX_PRINT_PATH_BYTES || !pdf || path.chars().any(char::is_control) {
        return Err("path");
    }
    Ok(GuestJob { path, bytes })
}

/// The job coming over.
struct JobIn {
    job: GuestJob,
    part: PathBuf,
    file: std::fs::File,
    sha: Sha256,
    /// Its first bytes, up to the PDF header's length.
    head: Vec<u8>,
    /// Bytes written: where the next range starts.
    next: u64,
    /// The guest file's time as its first range gave it; every later range must too.
    mtime: Option<String>,
    /// The range coming in: its length, the bytes and the GETCHUNKs seen.
    range: Option<(u64, u64, u64)>,
}

#[derive(Default)]
pub(super) struct PrintJobs {
    /// Where finished jobs go; None: this probe takes no print jobs.
    dir: Option<PathBuf>,
    /// The app's last PRINTWATCH through this harness: Some(true) on. A restarted agent is asked again.
    pub(super) watch: Option<bool>,
    /// The agent's READY carried `print`.
    pub(super) agent: bool,
    waiting: VecDeque<GuestJob>,
    current: Option<JobIn>,
    done: VecDeque<String>,
}

impl PrintJobs {
    pub(super) fn new(dir: Option<PathBuf>) -> Self {
        Self { dir, ..Self::default() }
    }

    /// From the environment; the first harness of this process removes the temporaries an earlier
    /// probe left.
    pub(super) fn from_env() -> Self {
        let dir = std::env::var_os(PRINT_DIR_ENV).filter(|dir| !dir.is_empty()).map(PathBuf::from);
        if let Some(dir) = dir.as_deref() {
            print_generation();
            if !CLEARED.swap(true, Ordering::Relaxed) {
                clear_jobs(dir);
            }
        }
        Self::new(dir)
    }

    fn taking(&self) -> bool {
        self.dir.is_some() && self.watch == Some(true)
    }

    fn known(&self, path: &str) -> bool {
        self.waiting.iter().any(|job| job.path == path) || self.current.as_ref().is_some_and(|c| c.job.path == path)
    }

    fn count(&self) -> usize {
        self.waiting.len() + usize::from(self.current.is_some())
    }

    fn remember(&mut self, path: String) {
        if self.done.len() == DONE_KEPT {
            self.done.pop_front();
        }
        self.done.push_back(path);
    }

    /// Drop the job under way: its file goes, nothing is published.
    fn abandon_current(&mut self) {
        if let Some(current) = self.current.take() {
            drop(current.file);
            let _ = std::fs::remove_file(current.part);
        }
    }
}

/// The `.pdf.part` temporaries an earlier probe left in `dir`. Its published `.pdf` jobs stay: the app
/// may still have them queued for a print panel, and their guest copies are deleted.
fn clear_jobs(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let ours = name
            .strip_suffix(".pdf.part")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit() || b == b'-'));
        if ours && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A fresh `<dir>/<start>-<n>.pdf.part` in a 0700 directory that is no link.
fn create_part(dir: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    if !std::fs::symlink_metadata(dir)?.is_dir() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let n = NEXT_JOB.fetch_add(1, Ordering::Relaxed);
    let part = dir.join(format!("{}-{n}.pdf.part", print_generation()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&part)?;
    Ok((part, file))
}

pub(super) fn is_print_req(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::PrintRange | ServiceReq::PrintDel { .. })
}

impl AgentConsoleHarness {
    /// The app's PRINTWATCH on its way out: remembered for a restarted agent, and held back from an
    /// agent without `print` (an update brings one, which is then asked). False: not sent now.
    pub(super) fn note_print_watch(&mut self, command: &str) -> bool {
        let Some(on) = print_watch_command(command) else {
            return true;
        };
        self.print.watch = Some(on);
        self.print.agent
    }

    /// A restarted agent has no watch: ask again if the app had asked.
    pub(super) fn resume_print_watch(&mut self) {
        let watch = |req: &ServiceReq| matches!(req, ServiceReq::Ctl(c) if print_watch_command(c).is_some());
        if self.print.watch == Some(true) && self.print.agent && !self.queue.iter().any(watch) {
            self.queue.push_back(ServiceReq::Ctl(PRINT_WATCH_VERB.to_owned()));
        }
    }

    /// The new agent announces again what the old one had not handed over.
    pub(super) fn print_agent_restarted(&mut self) {
        self.print.abandon_current();
        self.print.waiting.clear();
        self.queue.retain(|req| !matches!(req, ServiceReq::PrintRange));
    }

    /// A `PRINTJOB` line between replies; false for any other line.
    pub(super) fn print_job_announced(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix("PRINTJOB ") else {
            return false;
        };
        if !self.print.taking() {
            return true;
        }
        let job = match parse_print_job(rest) {
            Ok(job) => job,
            Err(reason) => {
                print_line(refused_line(reason, 0));
                return true;
            }
        };
        if self.print.done.contains(&job.path) {
            // Ended here already; its DEL was lost with an agent restart.
            self.queue.push_back(ServiceReq::PrintDel { path: job.path });
            return true;
        }
        if self.print.known(&job.path) {
            return true;
        }
        let refusal = if job.bytes == 0 {
            Some("empty")
        } else if job.bytes > MAX_PRINT_JOB_BYTES {
            Some("too-large")
        } else if self.print.count() >= MAX_PRINT_JOBS {
            Some("too-many")
        } else {
            None
        };
        if let Some(reason) = refusal {
            print_line(refused_line(reason, job.bytes));
            self.print.remember(job.path.clone());
            self.queue.push_back(ServiceReq::PrintDel { path: job.path });
            return true;
        }
        self.print.waiting.push_back(job);
        self.next_print_job();
        true
    }

    /// Queue the next range when nothing of a job is queued or on the wire.
    fn next_print_job(&mut self) {
        let busy = self.any_pending(|req| matches!(req, ServiceReq::PrintRange));
        if !busy && (self.print.current.is_some() || !self.print.waiting.is_empty()) {
            self.queue.push_back(ServiceReq::PrintRange);
        }
    }

    /// End the job under way with its one line, delete its guest file and go on.
    pub(super) fn fail_print_job(&mut self, reason: &str) {
        let Some(current) = self.print.current.as_ref() else {
            return;
        };
        let job = current.job.clone();
        self.print.abandon_current();
        print_line(refused_line(reason, job.bytes));
        self.print.remember(job.path.clone());
        self.queue.push_back(ServiceReq::PrintDel { path: job.path });
        self.next_print_job();
    }

    /// `GETRANGE <b64(path)> <offset> <length>` for the job under way, which starts here (its file
    /// is made) when none is. False: nothing to send.
    pub(super) fn write_print_range_line(&mut self) -> bool {
        while self.print.current.is_none() {
            let (Some(job), Some(dir)) = (self.print.waiting.pop_front(), self.print.dir.clone()) else {
                return false;
            };
            match create_part(&dir) {
                Ok((part, file)) => {
                    self.print.current = Some(JobIn {
                        job,
                        part,
                        file,
                        sha: Sha256::new(),
                        head: Vec::new(),
                        next: 0,
                        mtime: None,
                        range: None,
                    });
                }
                Err(_) => {
                    print_line(refused_line("write-error", job.bytes));
                    self.print.remember(job.path.clone());
                    self.queue.push_back(ServiceReq::PrintDel { path: job.path });
                }
            }
        }
        let current = self.print.current.as_ref().expect("checked");
        let len = (current.job.bytes - current.next).min(SHARE_RANGE_BYTES);
        self.service_line_scratch.push_str("GETRANGE ");
        base64_encode_into(current.job.path.as_bytes(), &mut self.service_line_scratch);
        let _ = writeln!(self.service_line_scratch, " {} {len}", current.next);
        true
    }

    /// `DEL <b64(path)>`: the guest's copy of a job that ended here.
    pub(super) fn write_print_del_line(&mut self, path: &str) -> bool {
        self.service_line_scratch.push_str("DEL ");
        base64_encode_into(path.as_bytes(), &mut self.service_line_scratch);
        self.service_line_scratch.push('\n');
        true
    }

    /// A line of a GETRANGE reply while a job's range is on the wire; false for any other. The
    /// reply is read to its GETEND even when its job ended meanwhile.
    pub(super) fn handle_print_line(&mut self, line: &str) -> bool {
        if !matches!(self.in_flight, Some((ServiceReq::PrintRange, _))) {
            return false;
        }
        let (beg, chunk) = (line.strip_prefix("GETBEG "), line.strip_prefix("GETCHUNK "));
        let end = line.starts_with("GETEND ");
        if beg.is_none() && chunk.is_none() && !end && !line.starts_with("ERR") {
            return false;
        }
        if beg.is_none() && chunk.is_none() {
            self.complete_in_flight();
        }
        if self.print.current.is_none() {
            // The job ended mid-reply; the wire is free at its end.
            if beg.is_none() && chunk.is_none() {
                self.next_print_job();
            }
            return true;
        }
        let failed = match (beg, chunk) {
            (Some(rest), _) => self.print_range_begin(rest),
            (_, Some(rest)) => self.print_range_chunk(rest),
            _ if end => self.print_range_end(),
            _ => Some("get-error"),
        };
        if let Some(reason) = failed {
            self.fail_print_job(reason);
        }
        true
    }

    /// GETBEG `<b64(path)> <bytes> <nchunks> <offset> <size> <mtime>`: the range asked for, of the
    /// announced file at its announced size, unchanged since its first range.
    fn print_range_begin(&mut self, rest: &str) -> Option<&'static str> {
        let current = self.print.current.as_mut()?;
        let Some((path, len, range)) = parse_range_beg(rest) else {
            return Some("mismatch");
        };
        let expected = (current.job.bytes - current.next).min(SHARE_RANGE_BYTES);
        let same = path == current.job.path
            && range.offset == current.next
            && range.size == current.job.bytes
            && current.mtime.as_ref().is_none_or(|mtime| *mtime == range.mtime)
            && len == expected
            && current.range.is_none();
        if !same {
            return Some("changed");
        }
        current.mtime = Some(range.mtime);
        current.range = Some((len, 0, 0));
        None
    }

    /// GETCHUNK `<seq> <b64>`: the range's next bytes, hashed and written at once.
    fn print_range_chunk(&mut self, rest: &str) -> Option<&'static str> {
        let current = self.print.current.as_mut()?;
        let Some((len, seen, seq)) = current.range.as_mut() else {
            return Some("mismatch");
        };
        let bytes = rest
            .split_once(' ')
            .filter(|(number, _)| number.parse() == Ok(*seq))
            .and_then(|(_, b64)| base64_decode(b64).ok())
            .filter(|bytes| *seen + bytes.len() as u64 <= *len);
        let Some(bytes) = bytes else {
            return Some("mismatch");
        };
        if current.file.write_all(&bytes).is_err() {
            return Some("write-error");
        }
        *seen += bytes.len() as u64;
        *seq += 1;
        current.sha.update(&bytes);
        let want = b"%PDF-".len().saturating_sub(current.head.len()).min(bytes.len());
        current.head.extend_from_slice(&bytes[..want]);
        None
    }

    /// GETEND: the range is whole; the job's next range, or the job is published.
    fn print_range_end(&mut self) -> Option<&'static str> {
        let current = self.print.current.as_mut()?;
        let Some((len, seen, _)) = current.range.take() else {
            return Some("mismatch");
        };
        if seen != len {
            return Some("short");
        }
        current.next += len;
        if current.next < current.job.bytes {
            self.next_print_job();
            return None;
        }
        if current.head != b"%PDF-" {
            return Some("not-pdf");
        }
        let current = self.print.current.take().expect("checked");
        let JobIn { job, part, file, sha, .. } = current;
        drop(file);
        let published = part.with_extension("");
        if std::fs::rename(&part, &published).is_err() {
            let _ = std::fs::remove_file(&part);
            print_line(refused_line("write-error", job.bytes));
        } else {
            let digest: [u8; 32] = sha.finalize().into();
            print_line(format!(
                "BVPRINT job bytes={} sha256={} path={}",
                job.bytes,
                hex_lower(&digest),
                published.display()
            ));
        }
        self.print.remember(job.path.clone());
        self.queue.push_back(ServiceReq::PrintDel { path: job.path });
        self.next_print_job();
        None
    }

    pub(super) fn handle_print_del_reply(&mut self, line: &str) {
        if line.starts_with("OK DEL") || line.starts_with("ERR") {
            self.complete_in_flight();
        }
    }
}
