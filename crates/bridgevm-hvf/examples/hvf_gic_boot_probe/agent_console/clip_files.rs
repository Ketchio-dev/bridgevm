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

use super::*;
use super::host_pasteboard::PasteboardOwner;
use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// The agent serves CLIPXF (with CF_HTML).
pub(super) const CLIP_CAPS_FILES_MARKER: &str = "CF_HDROP";
/// The agent serves CLIPSETFILES (with CF_HDROP): Mac file copies go to Windows.
pub(super) const CLIP_CAPS_SET_FILES_MARKER: &str = "CLIPSETFILES";
/// Most entries (files and folders) one copy brings over: the agent's FilesMax.
pub(super) const CLIP_FILES_MAX_ENTRIES: usize = 256;
/// Most file bytes one copy brings over.
pub(super) const CLIP_FILES_MAX_BYTES: u64 = 64 << 20;
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
        Ok(count) if list == "-" || count > CLIP_FILES_MAX_ENTRIES => GuestFiles::Unlisted(count),
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
    bytes: usize,
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
            let bytes = usize::try_from(entry.bytes).ok()?;
            plan.files.push_back(StagedFile { guest: entry.path.clone(), rel, bytes });
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
}

impl ClipFilesStage {
    pub(super) fn new(dir: PathBuf) -> Self {
        Self { dir, pull: None, published: VecDeque::new() }
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

/// A new file (never an existing one, never through a link) with `bytes`.
fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?
        .write_all(bytes)
}

/// The refusal record: counts and sizes only, never a name.
pub(super) fn guest_files_refusal_line(reason: &str, entries: usize, bytes: u64, copy: u64, t_ms: u128) -> String {
    format!("BVAGENT CLIPSYNC refused guest-files {reason} files={entries} bytes={bytes} copy={copy} t={t_ms}")
}

impl AgentConsoleHarness {
    /// The files part of a CLIPXF reply. True when the copy is a files copy
    /// and was handled here; false sends it on as text and image.
    pub(super) fn handle_guest_files(&mut self, files: GuestFiles, part: &str, later: bool, now: Instant) -> bool {
        let listed = match files {
            GuestFiles::Listed(list) => Ok(list),
            GuestFiles::Unlisted(count) => Err(count),
            GuestFiles::None | GuestFiles::Malformed => {
                // A newer copy without files ends any transfer.
                if self.last_synced.files.take().is_some() {
                    self.abandon_files_pull();
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
        let list = match listed {
            Ok(list) => list,
            Err(count) => {
                clip_line(guest_files_refusal_line("too-large", count, 0, copy, self.t_ms(now)));
                return true;
            }
        };
        let bytes = list.iter().filter(|file| !file.dir).fold(0u64, |sum, file| sum.saturating_add(file.bytes));
        if list.len() > CLIP_FILES_MAX_ENTRIES || bytes > CLIP_FILES_MAX_BYTES {
            clip_line(guest_files_refusal_line("too-large", list.len(), bytes, copy, self.t_ms(now)));
            return true;
        }
        let Some(plan) = plan_guest_files(&list) else {
            clip_line(guest_files_refusal_line("unsafe-path", list.len(), bytes, copy, self.t_ms(now)));
            return true;
        };
        self.start_files_pull(plan, list.len(), bytes, digest, now);
        true
    }

    /// Stage the copy's folders, then fetch its files, next on the wire.
    fn start_files_pull(&mut self, plan: FilesPlan, entries: usize, bytes: u64, digest: [u8; 32], now: Instant) {
        let copy = self.last_synced.guest_copy;
        let Ok(staging) = create_staging(&self.clip_files_stage.dir, copy) else {
            let t_ms = self.t_ms(now);
            return clip_line(guest_files_refusal_line("staging-error", entries, bytes, copy, t_ms));
        };
        let made = plan
            .dirs
            .iter()
            .try_for_each(|dir| std::fs::DirBuilder::new().recursive(true).create(staging.join(dir)));
        self.clip_files_stage.pull = Some(FilesPull {
            digest,
            copy,
            staging,
            pending: plan.files,
            tops: plan.tops,
            entries,
            bytes,
            owner: self.pasteboard.as_ref().map(HostPasteboard::owner),
        });
        if made.is_err() {
            return self.fail_files_pull("staging-error", now);
        }
        self.next_files_pull(now, true);
    }

    /// Queue the next fetch, or publish the copy when every file is in.
    fn next_files_pull(&mut self, now: Instant, first: bool) {
        let Some(pull) = self.clip_files_stage.pull.take_if(|pull| pull.pending.is_empty()) else {
            // Later fetches take turns with the polls that may bring a newer copy.
            if first {
                self.queue.push_front(ServiceReq::ClipFilesPull);
            } else {
                self.queue.push_back(ServiceReq::ClipFilesPull);
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
        self.queue.retain(|req| !matches!(req, ServiceReq::ClipFilesPull));
        if let Some(pull) = self.clip_files_stage.pull.take() {
            let _ = std::fs::remove_dir_all(pull.staging);
        }
    }

    fn fail_files_pull(&mut self, reason: &str, now: Instant) {
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
        self.service_line_scratch.push_str("GET ");
        base64_encode_into(file.guest.as_bytes(), &mut self.service_line_scratch);
        self.service_line_scratch.push('\n');
        true
    }

    /// GETBEG/GETCHUNK accumulate as for any GET; GETEND writes the file when
    /// it is the listed path and size.
    pub(super) fn handle_clip_files_pull_reply(&mut self, line: &str, now: Instant) {
        let error = line.starts_with("ERR");
        if !error && !line.starts_with("GETEND ") {
            return;
        }
        let Some((ServiceReq::ClipFilesPull, _)) = self.in_flight.take() else {
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
            Some(get) if get.path == file.guest && get.total == file.bytes && get.bytes.len() == file.bytes => {
                write_new_file(&pull.staging.join(&file.rel), &get.bytes).map_err(|_| "write-error")
            }
            _ => Err("mismatch"),
        };
        match written {
            Ok(()) => self.next_files_pull(now, false),
            Err(reason) => self.fail_files_pull(reason, now),
        }
    }
}
