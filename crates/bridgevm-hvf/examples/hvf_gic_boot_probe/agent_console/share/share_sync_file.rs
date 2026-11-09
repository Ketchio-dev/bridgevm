//! The share's synced state on disk (share_synced.rs), beside the VM's own UEFI vars:
//! `<vars>.share-sync` holds each name's last and previous synced version and the marks as of a
//! sequence number; `<vars>.share-sync-puts` holds the marks as of every later number.
//!
//! A mark reaches the disk before what it guards: before a PUT goes out (on the service thread)
//! and before a fetched copy is published (on whichever thread publishes it): a few names,
//! renamed into place and ordered with F_BARRIERFSYNC before any later write, as a guest flush is
//! ordered. One writer, behind a lock, keeps the marks file and its number. The versions are
//! written by a thread of their own about once a second (and at session end), with the marks as
//! they are then: settled marks leave the disk that way. Losing that write is safe: the marks file
//! then still holds every mark it has not dropped, and an old version only makes the Mac win. A
//! file that is missing, unreadable or for another Mac folder is ignored (the share works as
//! before), and a mark that cannot be written turns the state off and deletes it, so no session
//! trusts a state that missed one.

use super::*;
use share_sync::share_synced::{SyncedFile, SyncedVersion};
use std::collections::HashMap;
use std::io::Write as _;
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::sync::{mpsc, Arc, Mutex};

const VERSIONS_HEADER: &str = "bridgevm-share-sync 1";
const MARKS_HEADER: &str = "bridgevm-share-puts 1";
/// The versions are written at most this often.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

enum ToWriter {
    Flush { changes: Vec<(String, Option<SyncedFile>)>, marks: Vec<String>, seq: u64 },
    /// The state is off: delete both files after any write still queued.
    Remove,
}

/// The marks file, its number and the names it holds (less those settled since).
pub(in crate::agent_console) struct Marks {
    path: PathBuf,
    root: String,
    seq: u64,
    names: HashSet<String>,
    /// A mark could not be written: the state must go.
    failed: bool,
}

pub(in crate::agent_console) type SharedMarks = Arc<Mutex<Marks>>;

impl Marks {
    /// `name` marked on disk, written now unless it already is. An error also sets `failed`.
    pub(in crate::agent_console) fn add(&mut self, name: &str) -> std::io::Result<()> {
        if self.names.contains(name) {
            return Ok(());
        }
        self.names.insert(name.to_string());
        self.seq += 1;
        let mut text = format!("{MARKS_HEADER}\nroot {}\nseq {}\n", self.root, self.seq);
        push_marks(&mut text, self.names.iter());
        let written = write_replace(&self.path, text.as_bytes()).and_then(|file| {
            // SAFETY: a live descriptor; F_BARRIERFSYNC takes no argument.
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
        self.failed |= written.is_err();
        written
    }
}

/// Mark `name` on disk through the shared writer (from any thread).
pub(in crate::agent_console) fn mark(marks: &SharedMarks, name: &str) -> std::io::Result<()> {
    marks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).add(name)
}

pub(in crate::agent_console) struct SyncStateFile {
    versions: PathBuf,
    marks: SharedMarks,
    root: String,
    /// The versions loaded at the start, until the writer thread takes them.
    base: Option<HashMap<String, SyncedFile>>,
    writer: Option<mpsc::Sender<ToWriter>>,
    thread: Option<std::thread::JoinHandle<()>>,
    last_flush: Option<Instant>,
}

type Loaded = (HashMap<String, SyncedFile>, HashSet<String>, u64);

impl SyncStateFile {
    /// The state beside `vars` for the Mac folder `host_dir`, and what it holds when it can be
    /// trusted (otherwise nothing: every decision is then the one without a state).
    pub(in crate::agent_console) fn open(vars: &Path, host_dir: &Path) -> (Self, HashMap<String, SyncedFile>, HashSet<String>) {
        let side = |suffix: &str| {
            let mut name = vars.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        };
        let mut root = String::new();
        base64_encode_into(host_dir.as_os_str().as_bytes(), &mut root);
        let mut file = Self {
            versions: side(".share-sync"),
            marks: Arc::new(Mutex::new(Marks {
                path: side(".share-sync-puts"),
                root: root.clone(),
                seq: 0,
                names: HashSet::new(),
                failed: false,
            })),
            root,
            base: None,
            writer: None,
            thread: None,
            last_flush: None,
        };
        let (files, marks, seq) = file.load().unwrap_or_else(|| {
            println!("BVAGENT SHARE sync-state ignored");
            Default::default()
        });
        {
            let mut held = file.lock();
            held.seq = seq;
            held.names = marks.clone();
        }
        file.base = Some(files.clone());
        (file, files, marks)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The marks writer, for a thread that publishes a fetched copy.
    pub(in crate::agent_console) fn marks(&self) -> SharedMarks {
        Arc::clone(&self.marks)
    }

    fn load(&self) -> Option<Loaded> {
        let read = |path: &Path| match std::fs::read_to_string(path) {
            Ok(text) => Some(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(None),
            Err(_) => None,
        };
        let marks_path = self.lock().path.clone();
        let (versions, marks) = (read(&self.versions)?, read(&marks_path)?);
        let (files, mut marked, seq) = match versions {
            Some(text) => parse(&text, VERSIONS_HEADER, &self.root)?,
            None => Default::default(),
        };
        let (later, later_seq) = match marks {
            Some(text) => parse(&text, MARKS_HEADER, &self.root).map(|(_, marks, seq)| (marks, seq))?,
            None => Default::default(),
        };
        if later_seq > seq {
            marked.extend(later);
        }
        Some((files, marked, seq.max(later_seq)))
    }

    /// Mark `name` on disk before a PUT goes out or a fetched copy is published here.
    pub(in crate::agent_console) fn mark(&self, name: &str) -> std::io::Result<()> {
        self.lock().add(name)
    }

    /// Settled marks leave, and the versions changed since the last flush go to the writer, at
    /// most once a second. False when a mark could not be written: the caller turns the state off.
    pub(in crate::agent_console) fn flush(&mut self, engine: &mut ShareSync, now: Instant) -> bool {
        if self.lock().failed {
            return false;
        }
        if self.last_flush.is_some_and(|at| now.duration_since(at) < FLUSH_EVERY) {
            return true;
        }
        let expired = engine.expire_marks(now);
        if !expired.is_empty() {
            let mut held = self.lock();
            for name in &expired {
                held.names.remove(name);
            }
        }
        self.flush_now(engine, now);
        true
    }

    /// The session ends (a guest restart, the VM stopping): what is left, then the writer ends.
    pub(in crate::agent_console) fn finish(&mut self, engine: &mut ShareSync) {
        self.flush_now(engine, Instant::now());
        self.writer = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn flush_now(&mut self, engine: &mut ShareSync, now: Instant) {
        let Some(changes) = engine.take_synced_changes() else {
            return;
        };
        self.last_flush = Some(now);
        // The marks as on disk, under the lock: a mark another thread just wrote is kept.
        let (marks, seq) = {
            let held = self.lock();
            (held.names.iter().cloned().collect(), held.seq)
        };
        let message = ToWriter::Flush { changes, marks, seq };
        if let Some(writer) = self.writer() {
            let _ = writer.send(message);
        }
    }

    fn writer(&mut self) -> Option<&mpsc::Sender<ToWriter>> {
        if self.writer.is_none() {
            let (to, inbox) = mpsc::channel();
            let (versions, marks, root) = (self.versions.clone(), self.lock().path.clone(), self.root.clone());
            let files = self.base.take().unwrap_or_default();
            let thread = std::thread::Builder::new()
                .name("bvagent-share-state".into())
                .spawn(move || run(&versions, &marks, &root, files, &inbox))
                .ok()?;
            self.writer = Some(to);
            self.thread = Some(thread);
        }
        self.writer.as_ref()
    }

    /// The state is off for this session: delete it, so no later session trusts it.
    pub(in crate::agent_console) fn remove(mut self) {
        let _ = std::fs::remove_file(&self.lock().path);
        let _ = std::fs::remove_file(&self.versions);
        if let Some(writer) = self.writer.take() {
            let _ = writer.send(ToWriter::Remove);
        }
        drop(self.thread.take());
    }
}

fn run(versions: &Path, marks: &Path, root: &str, mut files: HashMap<String, SyncedFile>, inbox: &mpsc::Receiver<ToWriter>) {
    let mut warned = false;
    while let Ok(message) = inbox.recv() {
        let ToWriter::Flush { changes, marks: marked, seq } = message else {
            let _ = std::fs::remove_file(marks);
            let _ = std::fs::remove_file(versions);
            return;
        };
        for (name, file) in changes {
            match file {
                Some(file) => files.insert(name, file),
                None => files.remove(&name),
            };
        }
        let mut text = format!("{VERSIONS_HEADER}\nroot {root}\nseq {seq}\n");
        push_marks(&mut text, marked.iter());
        let mut b64 = String::new();
        for (name, file) in &files {
            b64.clear();
            base64_encode_into(name.as_bytes(), &mut b64);
            let prev = file.prev.map_or("- -".to_string(), |v| format!("{} {}", v.size, v.mtime_ms));
            let _ = writeln!(text, "file {b64} {} {} {prev}", file.last.size, file.last.mtime_ms);
        }
        if let Err(error) = write_replace(versions, text.as_bytes()) {
            if !warned {
                println!("BVAGENT SHARE sync-state unwritable {error}");
                warned = true;
            }
        }
    }
}

fn push_marks<'a>(text: &mut String, marks: impl Iterator<Item = &'a String>) {
    for name in marks {
        text.push_str("mark ");
        base64_encode_into(name.as_bytes(), text);
        text.push('\n');
    }
}

/// A temporary beside `path`, renamed over it; the file, still open, for a barrier.
fn write_replace(path: &Path, bytes: &[u8]) -> std::io::Result<std::fs::File> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    std::fs::rename(&temporary, path)?;
    Ok(file)
}

/// A state file's versions, marks and number; None when it is not one for `root`.
fn parse(text: &str, header: &str, root: &str) -> Option<Loaded> {
    let mut lines = text.lines();
    if lines.next()? != header || lines.next()?.strip_prefix("root ")? != root {
        return None;
    }
    let seq = lines.next()?.strip_prefix("seq ")?.parse().ok()?;
    let name = |b64: &str| String::from_utf8(base64_decode(b64).ok()?).ok();
    let (mut files, mut marks) = (HashMap::new(), HashSet::new());
    for line in lines {
        let mut it = line.split(' ');
        match it.next()? {
            "mark" => {
                marks.insert(name(it.next()?)?);
            }
            "file" => {
                let key = name(it.next()?)?;
                let last = SyncedVersion { size: it.next()?.parse().ok()?, mtime_ms: it.next()?.parse().ok()? };
                let prev = match (it.next()?, it.next()?) {
                    ("-", "-") => None,
                    (size, mtime_ms) => Some(SyncedVersion { size: size.parse().ok()?, mtime_ms: mtime_ms.parse().ok()? }),
                };
                files.insert(key, SyncedFile { last, prev });
            }
            _ => return None,
        }
    }
    Some((files, marks, seq))
}
