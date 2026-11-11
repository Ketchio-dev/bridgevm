//! Files and folders copied in Windows Explorer (CF_HDROP), with an agent
//! that answered CLIPCAPS with CF_HDROP. CLIPXF polls carry the copy's file
//! list, each file comes over with the shared folder's GET, one at a time,
//! and the copy's top-level items reach the Mac pasteboard as file URLs,
//! which Finder pastes.
//!
//! The list and every GET are guest data. Each entry's path below its copied
//! item is checked part by part, files are written only inside a fresh 0700
//! staging directory, and each GET must name the listed path and size. Over a
//! limit, or with a path that could leave the staging directory, the copy
//! stays in Windows with one line.
//!
//! A copy past the old limits (`CLIP_FILES_MAX_*`), from an agent whose READY
//! carries `bigfile`, comes over with the shared folder's GETRANGE
//! (share_big.rs) instead: each range is its own request, so clipboard polls,
//! control and shared-folder traffic go between the ranges, and each GETCHUNK
//! is written to its file as it comes, so memory holds one chunk. Every range
//! must carry the listed size and the time of the file's first range: a file
//! changed while it was copied fails the copy. The Mac needs room for the copy
//! plus the shared folder's margin first.

use super::*;
use super::host_pasteboard::PasteboardOwner;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// The agent serves CLIPXF (with CF_HTML).
pub(super) const CLIP_CAPS_FILES_MARKER: &str = "CF_HDROP";
/// The agent serves CLIPSETFILES (with CF_HDROP): Mac file copies go to Windows.
pub(super) const CLIP_CAPS_SET_FILES_MARKER: &str = "CLIPSETFILES";
/// Most entries (files and folders) one copy brings over with whole-file GETs and PUTs (an agent
/// without `bigfile`): its FilesMax.
pub(super) const CLIP_FILES_MAX_ENTRIES: usize = 256;
/// Most file bytes one copy brings over with whole-file GETs and PUTs.
pub(super) const CLIP_FILES_MAX_BYTES: u64 = 64 << 20;
/// Most entries one copy brings over in ranges and chunks (an agent with `bigfile`): its FilesMax.
pub(super) const CLIP_FILES_BIG_MAX_ENTRIES: usize = 4096;
/// Most file bytes one copy brings over in ranges and chunks: the shared folder's largest file.
pub(super) const CLIP_FILES_BIG_MAX_BYTES: u64 = 16 << 30;
/// Published copies kept for a paste still reading them: at most this many, each this long.
pub(super) const CLIP_FILES_KEEP_COPIES: usize = 4;
const CLIP_FILES_KEEP_FOR: Duration = Duration::from_secs(10 * 60);

/// One line of a CLIPXF list: `<isDir>|<top>|<bytes>|<full Windows path>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GuestFile {
    pub(super) dir: bool,
    /// A copied item; the others are below the copied folder before them.
    pub(super) top: bool,
    pub(super) bytes: u64,
    pub(super) path: String,
}

/// The files part of a CLIPXF reply: `<file-count> <b64(list)|->`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GuestFiles {
    None,
    /// Not listed (over the agent's limit): its count only.
    Unlisted(usize),
    Listed(Vec<GuestFile>),
    Malformed,
}

/// `<file-count> <b64(list)|-> <rest>`: the files, the part as on the wire,
/// and the rest; None when a field is missing.
pub(super) fn parse_files_part(rest: &str) -> Option<(GuestFiles, &str, &str)> {
    let mut fields = rest.splitn(3, ' ');
    let (count, list, text) = (fields.next()?, fields.next()?, fields.next()?);
    let part = &rest[..count.len() + 1 + list.len()];
    let files = match count.parse::<usize>() {
        Ok(0) => GuestFiles::None,
        Ok(count) if list == "-" || count > CLIP_FILES_BIG_MAX_ENTRIES => GuestFiles::Unlisted(count),
        Ok(count) => decode_file_list(list, count).map_or(GuestFiles::Malformed, GuestFiles::Listed),
        Err(_) => GuestFiles::Malformed,
    };
    Some((files, part, text))
}

fn decode_file_list(b64: &str, count: usize) -> Option<Vec<GuestFile>> {
    let list = String::from_utf8(base64_decode(b64).ok()?).ok()?;
    let flag = |field: Option<&str>| match field? {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    };
    let files = list
        .strip_suffix('\n')?
        .split('\n')
        .map(|line| {
            // <isDir>|<top>|<bytes>|<mtime>|<path>: the mtime is only part of the copy's key (the
            // part's digest), so a same-size edit copied again is fetched again.
            let mut fields = line.splitn(5, '|');
            let (dir, top, bytes) = (flag(fields.next())?, flag(fields.next())?, fields.next()?.parse().ok()?);
            fields.next()?.parse::<u64>().ok()?;
            let path = fields.next().filter(|path| !path.is_empty())?.to_owned();
            Some(GuestFile { dir, top, bytes, path })
        })
        .collect::<Option<Vec<_>>>()?;
    (files.len() == count).then_some(files)
}

/// A listed file and where it goes below the staging directory.
pub(super) struct StagedFile {
    guest: String,
    rel: PathBuf,
    bytes: u64,
}

/// A listed copy laid out below its staging directory.
pub(super) struct FilesPlan {
    dirs: Vec<PathBuf>,
    files: VecDeque<StagedFile>,
    tops: Vec<PathBuf>,
}

/// Where each entry goes: a copied item by its name in its own numbered folder,
/// an entry below a copied folder by its path from that folder's parent (a byte-exact prefix plus
/// `\`). None when an entry is not below the copied folder before it, or a
/// part of its path is empty, `.`, `..`, or holds `/` or NUL.
pub(super) fn plan_guest_files(list: &[GuestFile]) -> Option<FilesPlan> {
    let mut plan = FilesPlan {
        dirs: Vec::new(),
        files: VecDeque::new(),
        tops: Vec::new(),
    };
    // The copied folder the entries after it belong to, where its name starts and its slot.
    let mut folder: Option<(&str, usize, usize)> = None;
    let mut slots = 0usize;
    for entry in list {
        let path = entry.path.as_str();
        // Each copied item stages in its own numbered folder: two items of one name (from two
        // Windows folders, as in search results) stay two items.
        let (name_at, slot) = if entry.top {
            let name_at = path.rfind('\\')? + 1;
            let slot = slots;
            slots += 1;
            plan.dirs.push(PathBuf::from(slot.to_string()));
            folder = entry.dir.then_some((path, name_at, slot));
            (name_at, slot)
        } else {
            let (top, name_at, slot) = folder?;
            path.strip_prefix(top)?.strip_prefix('\\')?;
            (name_at, slot)
        };
        let mut rel = PathBuf::from(slot.to_string());
        for part in path[name_at..].split('\\') {
            if part.is_empty() || part == "." || part == ".." || part.contains(['/', '\0']) {
                return None;
            }
            rel.push(part);
        }
        if entry.top {
            plan.tops.push(rel.clone());
        }
        if entry.dir {
            plan.dirs.push(rel);
        } else {
            plan.files.push_back(StagedFile { guest: entry.path.clone(), rel, bytes: entry.bytes });
        }
    }
    Some(plan)
}

/// The staging state: this process's directory, the transfer under way and
/// the copy the Mac pasteboard points at.
pub(super) struct ClipFilesStage {
    /// `<temp>/bridgevm-clip-files/<pid>`; each copy stages in `<copy>/` below.
    pub(super) dir: PathBuf,
    pub(super) pull: Option<FilesPull>,
    /// The copies published, oldest first: Finder may still be pasting from an earlier one.
    pub(super) published: VecDeque<(PathBuf, Instant)>,
    /// Room a copy fetched in ranges leaves free on the Mac volume beyond its own size.
    pub(super) free_margin: u64,
}

impl ClipFilesStage {
    pub(super) fn new(dir: PathBuf) -> Self {
        Self { dir, pull: None, published: VecDeque::new(), free_margin: SHARE_FREE_MARGIN }
    }

    /// This process's staging directory, after deleting those of processes
    /// that are gone.
    pub(super) fn for_this_process() -> Self {
        let root = std::env::temp_dir().join("bridgevm-clip-files");
        for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let pid = entry.file_name().to_str().and_then(|name| name.parse::<libc::pid_t>().ok());
            if pid.is_some_and(|pid| pid > 0 && !pid_alive(pid)) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        Self::new(root.join(std::process::id().to_string()))
    }
}

fn pid_alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let sent = unsafe { libc::kill(pid, 0) } == 0;
    sent || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// A copy's files on their way to the Mac.
pub(super) struct FilesPull {
    digest: [u8; 32],
    copy: u64,
    staging: PathBuf,
    /// The files still to fetch, the first one next.
    pending: VecDeque<StagedFile>,
    tops: Vec<PathBuf>,
    entries: usize,
    bytes: u64,
    /// The Mac pasteboard when the transfer began: any Mac copy since wins.
    owner: Option<PasteboardOwner>,
    /// GETRANGE, its chunks written as they come; else one GET per file.
    ranged: bool,
    /// The file coming in ranges, once its first range began.
    current: Option<FileIn>,
}

/// A file of a ranged pull, written as its ranges come.
struct FileIn {
    file: std::fs::File,
    /// Bytes written: where the next range starts.
    next: u64,
    /// The guest file's time as its first range gave it; every later range must too.
    mtime: Option<String>,
    /// The range coming in: its length, the bytes and the GETCHUNKs seen.
    range: Option<(u64, u64, u64)>,
}

/// A fresh, empty 0700 `<dir>/<copy>`, below directories that are no links.
fn create_staging(dir: &Path, copy: u64) -> std::io::Result<PathBuf> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    for path in [dir.parent(), Some(dir)].into_iter().flatten() {
        if !std::fs::symlink_metadata(path)?.is_dir() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
    }
    let staging = dir.join(copy.to_string());
    match std::fs::remove_dir_all(&staging) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    std::fs::DirBuilder::new().mode(0o700).create(&staging)?;
    Ok(staging)
}

/// A new file (never an existing one, never through a link).
fn create_new_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

/// A new file (never an existing one, never through a link) with `bytes`.
fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    create_new_file(path)?.write_all(bytes)
}

/// The refusal record: counts and sizes only, never a name.
pub(super) fn guest_files_refusal_line(reason: &str, entries: usize, bytes: u64, copy: u64, t_ms: u128) -> String {
    format!("BVAGENT CLIPSYNC refused guest-files {reason} files={entries} bytes={bytes} copy={copy} t={t_ms}")
}

/// A file copy over the limits either way (`refused guest-files` or `skip host-files`), with the
/// limits the app's notice names.
pub(super) fn files_too_large_line(what: &str, entries: usize, bytes: u64, limits: (usize, u64), copy: u64, t_ms: u128) -> String {
    let (max_entries, max_bytes) = limits;
    format!(
        "BVAGENT CLIPSYNC {what} too-large files={entries} bytes={bytes} limit-files={max_entries} \
         limit-bytes={max_bytes} copy={copy} t={t_ms}"
    )
}

/// The write error a full Mac volume gives, as the copy's refusal says it.
fn write_refusal(error: &std::io::Error) -> &'static str {
    if is_out_of_space(error) {
        "low-space"
    } else {
        "write-error"
    }
}

impl AgentConsoleHarness {
    /// The files part of a CLIPXF reply. True when the copy is a files copy
    /// and was handled here; false sends it on as text and image.
    pub(super) fn handle_guest_files(
        &mut self,
        files: GuestFiles,
        part: &str,
        later: bool,
        echo: bool,
        now: Instant,
    ) -> bool {
        let listed = match files {
            GuestFiles::Listed(list) => Ok(list),
            GuestFiles::Unlisted(count) => Err(count),
            GuestFiles::None | GuestFiles::Malformed => {
                // A newer copy without files ends any transfer. When the guest's file drop
                // gave way to text, that text is new to the Mac even if it reads as the
                // drop's names did.
                if self.last_synced.files.take().is_some() {
                    self.abandon_files_pull();
                    if matches!(files, GuestFiles::None) {
                        self.last_synced.text = None;
                    }
                }
                return false;
            }
        };
        let digest = html_digest(part);
        if self.last_synced.files == Some(digest) && !self.last_synced.rebaseline {
            if later {
                clip_line(format!(
                    "BVAGENT CLIPSYNC guest->host unchanged copy={} t={}",
                    self.last_synced.guest_copy,
                    self.t_ms(now)
                ));
            }
            return true;
        }
        // A Mac file copy we pushed (clip_files_push.rs) coming back: known, never pulled. An
        // agent that lists fewer entries than a push brought names only their count.
        if listed.as_ref().map_or(echo, |list| self.lists_own_push(list)) {
            self.abandon_files_pull();
            self.last_synced.files = Some(digest);
            self.last_synced.rebaseline = false;
            if echo {
                self.last_synced.guest_copy = self.last_synced.host_copy;
            }
            return true;
        }
        // A new copy: the guest holds these files alone, and an older transfer is stale.
        self.abandon_files_pull();
        self.last_synced.text = None;
        self.last_synced.image = None;
        self.last_synced.html = None;
        self.last_synced.files = Some(digest);
        if std::mem::take(&mut self.last_synced.rebaseline) {
            return true;
        }
        // As for images: the first poll after the update announces them again.
        if self.agent_update_holds_images() {
            self.last_synced.files = None;
            return true;
        }
        let copy = self.last_synced.guest_copy;
        let limits = self.clip_files_limits();
        let list = match listed {
            Ok(list) => list,
            Err(count) => {
                // The agent lists up to its FilesMax and then names the count it stopped at.
                let limits = (count.saturating_sub(1).clamp(1, limits.0), limits.1);
                clip_line(files_too_large_line("refused guest-files", count, 0, limits, copy, self.t_ms(now)));
                return true;
            }
        };
        let bytes = list.iter().filter(|file| !file.dir).fold(0u64, |sum, file| sum.saturating_add(file.bytes));
        if list.len() > limits.0 || bytes > limits.1 {
            clip_line(files_too_large_line("refused guest-files", list.len(), bytes, limits, copy, self.t_ms(now)));
            return true;
        }
        let Some(plan) = plan_guest_files(&list) else {
            clip_line(guest_files_refusal_line("unsafe-path", list.len(), bytes, copy, self.t_ms(now)));
            return true;
        };
        self.start_files_pull(plan, list.len(), bytes, digest, now);
        true
    }

    /// The most entries and bytes one copy may hold, either way, with this agent.
    pub(super) fn clip_files_limits(&self) -> (usize, u64) {
        if self.share_big {
            (CLIP_FILES_BIG_MAX_ENTRIES, CLIP_FILES_BIG_MAX_BYTES)
        } else {
            (CLIP_FILES_MAX_ENTRIES, CLIP_FILES_MAX_BYTES)
        }
    }

    /// Every copied item lies below `<clipdir>\files\`, where Mac copies go.
    fn lists_own_push(&self, list: &[GuestFile]) -> bool {
        let Some(images) = self.clip_images.as_ref() else {
            return false;
        };
        let own = format!("{}\\files\\", images.dir);
        let mut tops = list.iter().filter(|file| file.top).peekable();
        tops.peek().is_some() && tops.all(|file| file.path.starts_with(&own))
    }

    /// Stage the copy's folders, then fetch its files, next on the wire.
    fn start_files_pull(&mut self, plan: FilesPlan, entries: usize, bytes: u64, digest: [u8; 32], now: Instant) {
        let copy = self.last_synced.guest_copy;
        let Ok(staging) = create_staging(&self.clip_files_stage.dir, copy) else {
            let t_ms = self.t_ms(now);
            return clip_line(guest_files_refusal_line("staging-error", entries, bytes, copy, t_ms));
        };
        let ranged = self.share_big && over_old_limits(entries, bytes);
        let made = plan
            .dirs
            .iter()
            .try_for_each(|dir| std::fs::DirBuilder::new().recursive(true).create(staging.join(dir)));
        // As for a large shared-folder file: the copy and a margin must fit (share_big.rs).
        let free = std::fs::File::open(&staging).and_then(|dir| free_bytes(&dir)).unwrap_or(u64::MAX);
        let low_space = ranged && free < bytes.saturating_add(self.clip_files_stage.free_margin);
        self.clip_files_stage.pull = Some(FilesPull {
            digest,
            copy,
            staging,
            pending: plan.files,
            tops: plan.tops,
            entries,
            bytes,
            owner: self.pasteboard.as_ref().map(HostPasteboard::owner),
            ranged,
            current: None,
        });
        if made.is_err() {
            return self.fail_files_pull("staging-error", now);
        }
        if low_space {
            return self.fail_files_pull("low-space", now);
        }
        self.next_files_pull(now, true);
    }

    /// Queue the next fetch, or publish the copy when every file is in.
    fn next_files_pull(&mut self, now: Instant, first: bool) {
        let Some(pull) = self.clip_files_stage.pull.take_if(|pull| pull.pending.is_empty()) else {
            // Later fetches take turns with the polls that may bring a newer copy.
            let ranged = self.clip_files_stage.pull.as_ref().is_some_and(|pull| pull.ranged);
            if first {
                self.queue.push_front(ServiceReq::ClipFilesPull { ranged });
            } else {
                self.queue.push_back(ServiceReq::ClipFilesPull { ranged });
            }
            return;
        };
        let tops = pull.tops.iter().map(|top| pull.staging.join(top)).collect();
        if let (Some(pasteboard), Some(owner)) = (self.pasteboard.as_ref(), pull.owner) {
            pasteboard.set_files(tops, owner);
        }
        // Earlier copies stay a while: a paste into Finder (onto a slow disk, say) may still be
        // reading one. Kept: the newest few, each for some minutes.
        let published = &mut self.clip_files_stage.published;
        published.retain(|(dir, _)| *dir != pull.staging);
        published.push_back((pull.staging.clone(), now));
        while published.len() > CLIP_FILES_KEEP_COPIES
            || published.front().is_some_and(|(_, at)| now.duration_since(*at) >= CLIP_FILES_KEEP_FOR && published.len() > 1)
        {
            let (dir, _) = published.pop_front().expect("front was Some");
            let _ = std::fs::remove_dir_all(dir);
        }
        clip_line(format!(
            "BVAGENT CLIPSYNC guest->host files={} bytes={} copy={} t={}",
            pull.entries,
            pull.bytes,
            pull.copy,
            self.t_ms(now)
        ));
    }

    /// A newer guest copy, a Mac copy or an app paste came first.
    fn files_pull_current(&self) -> bool {
        self.clip_files_stage
            .pull
            .as_ref()
            .is_some_and(|pull| {
                self.last_synced.files == Some(pull.digest) && pull.owner == self.pasteboard.as_ref().map(HostPasteboard::owner)
            })
            && !self.last_synced.rebaseline
            && !self.clip_push_pending()
    }

    /// Drop the transfer: its queued fetches and its staging directory.
    pub(super) fn abandon_files_pull(&mut self) {
        self.queue.retain(|req| !matches!(req, ServiceReq::ClipFilesPull { .. }));
        if let Some(pull) = self.clip_files_stage.pull.take() {
            // The file being written closes first; nothing of the copy stays.
            drop(pull.current);
            let _ = std::fs::remove_dir_all(pull.staging);
        }
    }

    pub(super) fn fail_files_pull(&mut self, reason: &str, now: Instant) {
        let pull = self.clip_files_stage.pull.as_ref();
        let Some((copy, entries, bytes)) = pull.map(|pull| (pull.copy, pull.entries, pull.bytes)) else {
            return;
        };
        self.abandon_files_pull();
        clip_line(guest_files_refusal_line(reason, entries, bytes, copy, self.t_ms(now)));
    }

    /// GET the transfer's next file; false drops the request (the transfer is stale).
    pub(super) fn write_clip_files_line(&mut self) -> bool {
        if !self.files_pull_current() {
            self.abandon_files_pull();
            return false;
        }
        let Some(file) = self.clip_files_stage.pull.as_ref().and_then(|pull| pull.pending.front()) else {
            return false;
        };
        // A new GET cannot inherit an earlier request's fragments.
        self.get_accum = None;
        let pull = self.clip_files_stage.pull.as_ref().expect("checked");
        let out = &mut self.service_line_scratch;
        if pull.ranged {
            let next = pull.current.as_ref().map_or(0, |current| current.next);
            out.push_str("GETRANGE ");
            base64_encode_into(file.guest.as_bytes(), out);
            let _ = writeln!(out, " {next} {}", (file.bytes - next).min(SHARE_RANGE_BYTES));
        } else {
            out.push_str("GET ");
            base64_encode_into(file.guest.as_bytes(), out);
            out.push('\n');
        }
        true
    }

    /// GETBEG/GETCHUNK accumulate as for any GET; GETEND writes the file when
    /// it is the listed path and size.
    pub(super) fn handle_clip_files_pull_reply(&mut self, line: &str, now: Instant) {
        let error = line.starts_with("ERR");
        if !error && !line.starts_with("GETEND ") {
            return;
        }
        let Some((ServiceReq::ClipFilesPull { .. }, _)) = self.in_flight.take() else {
            return;
        };
        let finished = self.take_finished_get();
        if !self.files_pull_current() {
            return self.abandon_files_pull();
        }
        let Some(pull) = self.clip_files_stage.pull.as_mut() else {
            return;
        };
        let Some(file) = pull.pending.pop_front() else {
            return;
        };
        let written = match finished {
            _ if error => Err("get-error"),
            Some(get) if get.path == file.guest && get.total as u64 == file.bytes && get.bytes.len() as u64 == file.bytes => {
                write_new_file(&pull.staging.join(&file.rel), &get.bytes).map_err(|error| write_refusal(&error))
            }
            _ => Err("mismatch"),
        };
        match written {
            Ok(()) => self.next_files_pull(now, false),
            Err(reason) => self.fail_files_pull(reason, now),
        }
    }

    /// A line of a GETRANGE reply to a ranged pull; false leaves it to the GET path. The reply
    /// is read to its GETEND even when its copy was dropped meanwhile.
    pub(super) fn handle_clip_files_range_line(&mut self, line: &str, now: Instant) -> bool {
        if !matches!(self.in_flight, Some((ServiceReq::ClipFilesPull { ranged: true }, _))) {
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
        // A newer guest copy, a Mac copy or an app paste came first.
        if self.clip_files_stage.pull.is_some() && !self.files_pull_current() {
            self.abandon_files_pull();
            return true;
        }
        let failed = match (beg, chunk) {
            (Some(rest), _) => self.clip_range_begin(rest),
            (_, Some(rest)) => self.clip_range_chunk(rest),
            _ if end => self.clip_range_end(now),
            _ => Some("get-error"),
        };
        if let Some(reason) = failed {
            self.fail_files_pull(reason, now);
        }
        true
    }

    /// GETBEG `<b64(path)> <bytes> <nchunks> <offset> <size> <mtime>`: the range asked for, of the
    /// listed file, unchanged since its first range. The file is made at its first range.
    fn clip_range_begin(&mut self, rest: &str) -> Option<&'static str> {
        let pull = self.clip_files_stage.pull.as_mut()?;
        let file = pull.pending.front()?;
        let Some((path, len, range)) = parse_range_beg(rest) else {
            return Some("mismatch");
        };
        if pull.current.is_none() {
            match create_new_file(&pull.staging.join(&file.rel)) {
                Ok(created) => {
                    pull.current = Some(FileIn { file: created, next: 0, mtime: None, range: None });
                }
                Err(error) => return Some(write_refusal(&error)),
            }
        }
        let current = pull.current.as_mut()?;
        let expected = (file.bytes - current.next).min(SHARE_RANGE_BYTES);
        let same = path == file.guest
            && range.offset == current.next
            && range.size == file.bytes
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

    /// GETCHUNK `<seq> <b64>`: the range's next bytes, written to the file at once.
    fn clip_range_chunk(&mut self, rest: &str) -> Option<&'static str> {
        let pull = self.clip_files_stage.pull.as_mut()?;
        let Some(FileIn { file, range: Some((len, seen, seq)), .. }) = pull.current.as_mut() else {
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
        if let Err(error) = file.write_all(&bytes) {
            return Some(write_refusal(&error));
        }
        *seen += bytes.len() as u64;
        *seq += 1;
        None
    }

    /// GETEND: the range is whole; the file's next range, or the next file.
    fn clip_range_end(&mut self, now: Instant) -> Option<&'static str> {
        let pull = self.clip_files_stage.pull.as_mut()?;
        let bytes = pull.pending.front()?.bytes;
        let Some(current) = pull.current.as_mut() else {
            return Some("mismatch");
        };
        let Some((len, seen, _)) = current.range.take() else {
            return Some("mismatch");
        };
        if seen != len {
            return Some("short");
        }
        current.next += len;
        if current.next < bytes {
            self.queue.push_back(ServiceReq::ClipFilesPull { ranged: true });
            return None;
        }
        pull.current = None;
        pull.pending.pop_front();
        self.next_files_pull(now, false);
        None
    }
}
