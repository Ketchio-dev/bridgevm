//! The share's synced state on disk (share_synced.rs), beside the VM's own UEFI vars:
//! `<vars>.share-sync` holds each name's last and previous synced version and the marks as of a
//! sequence number; `<vars>.share-sync-puts` holds the marks as of every later number.
//!
//! A mark reaches the disk before what it guards: before a PUT goes out and before a fetched copy
//! is published: the names, renamed into place and ordered with F_BARRIERFSYNC before any later
//! write, as a guest flush is ordered. The service thread only asks for a PUT's marks; the state
//! thread writes them and the PUT waits for that number to be on disk (share_state.rs), so vCPU 0
//! never waits for the barrier. A fetched copy's mark is written by the thread that publishes it.
//! One write at a time, each with every mark asked before it, keeps the file's number in order. The versions are
//! written by a thread of their own about once a second (and at session end), with the marks as
//! they are then: settled marks leave the disk that way. Losing that write is safe: the marks file
//! then still holds every mark it has not dropped, and an old version only makes the Mac win. A
//! file that is missing, unreadable or for another Mac folder is ignored (the share works as
//! before), and a mark that cannot be written turns the state off, so no session trusts a state
//! that missed one: from then on no thread writes either file (each write checks under the write
//! lock), and the service thread removes both, retrying until it can; PUTs wait for that
//! (share_state.rs).

use super::*;
use share_sync::share_synced::{SyncedFile, SyncedVersion};
use std::collections::HashMap;
use std::io::Write as _;
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};

const VERSIONS_HEADER: &str = "bridgevm-share-sync 1";
const MARKS_HEADER: &str = "bridgevm-share-puts 1";
/// The versions are written at most this often.
const FLUSH_EVERY: Duration = Duration::from_secs(1);

enum ToWriter {
    Flush { changes: Vec<(String, Option<SyncedFile>)>, marks: Vec<String>, seq: u64 },
    /// The PUT marks asked for so far, to be written.
    Marks,
}

/// The marks asked for, their number and the names (less those settled since).
pub(in crate::agent_console) struct Marks {
    path: PathBuf,
    root: String,
    seq: u64,
    names: HashSet<String>,
}

/// The marks file, shared by the service thread (which asks), the state thread (which writes
/// PUT marks) and a thread that publishes a fetched copy (which writes its own).
pub(in crate::agent_console) struct MarksFile {
    state: Mutex<Marks>,
    /// Held while the file is written: one write at a time, each with every mark asked before it.
    write: Mutex<()>,
    /// The newest number on disk.
    durable: AtomicU64,
    /// A mark could not be written: the state must go.
    failed: AtomicBool,
    /// The state is off: nothing more is written, by any thread.
    off: AtomicBool,
    /// Tests hold the state thread's writes here.
    #[cfg(test)]
    pub(in crate::agent_console) hold: AtomicBool,
    /// Tests hold a marks write inside the write lock, past its `off` check (`parked` then).
    #[cfg(test)]
    pub(in crate::agent_console) hold_write: AtomicBool,
    #[cfg(test)]
    pub(in crate::agent_console) parked: AtomicBool,
}

pub(in crate::agent_console) type SharedMarks = Arc<MarksFile>;

impl MarksFile {
    fn lock(&self) -> MutexGuard<'_, Marks> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `names` asked for (no write here): the number that holds them all.
    pub(in crate::agent_console) fn ask<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> u64 {
        let mut held = self.lock();
        let before = held.names.len();
        held.names.extend(names.into_iter().map(str::to_string));
        if held.names.len() != before {
            held.seq += 1;
        }
        held.seq
    }

    /// Every mark asked so far on disk, unless it already is. An error also sets `failed`. Once
    /// the state is off nothing is written: a fetched copy is then published without its mark,
    /// which is safe, because the versions on disk predate it (a later session finds the Mac copy
    /// newer than its last synced version: the Mac wins).
    pub(in crate::agent_console) fn write_through(&self) -> std::io::Result<()> {
        // `off` is checked under this lock, which the write holds through its rename: once
        // `finish` or `invalidate` holds it after setting `off`, no write is under way or follows.
        let _one = self.write.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.off.load(Ordering::Acquire) {
            return Err(std::io::Error::other("the share state is off"));
        }
        #[cfg(test)]
        if self.hold_write.load(Ordering::Acquire) {
            self.parked.store(true, Ordering::Release);
            for _ in 0..5000 {
                if !self.hold_write.load(Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let (path, text, seq) = {
            let held = self.lock();
            if held.seq <= self.durable.load(Ordering::Acquire) {
                return Ok(());
            }
            let mut text = format!("{MARKS_HEADER}\nroot {}\nseq {}\n", held.root, held.seq);
            push_marks(&mut text, held.names.iter());
            (held.path.clone(), text, held.seq)
        };
        let written = write_replace(&path, text.as_bytes()).and_then(|file| {
            // SAFETY: a live descriptor; F_BARRIERFSYNC takes no argument.
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
        match &written {
            Ok(()) => drop(self.durable.fetch_max(seq, Ordering::AcqRel)),
            Err(_) => self.failed.store(true, Ordering::Release),
        }
        written
    }

    /// Number `seq` is on disk (Some(true)), not yet (Some(false)), or never will be (None).
    pub(in crate::agent_console) fn durable(&self, seq: u64) -> Option<bool> {
        if self.failed.load(Ordering::Acquire) {
            return None;
        }
        Some(self.durable.load(Ordering::Acquire) >= seq)
    }
}

/// Mark `names` on disk here (from any thread), in one write with any mark asked before.
pub(in crate::agent_console) fn mark<'a>(marks: &SharedMarks, names: impl IntoIterator<Item = &'a str>) -> std::io::Result<()> {
    marks.ask(names);
    marks.write_through()
}

pub(in crate::agent_console) struct SyncStateFile {
    versions: PathBuf,
    marks: SharedMarks,
    root: String,
    /// The versions loaded at the start, until the writer thread takes them.
    base: Option<HashMap<String, SyncedFile>>,
    writer: Option<mpsc::Sender<ToWriter>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// `invalidate` removed a file and its folder is not synced yet.
    removed_unsynced: bool,
    last_flush: Option<Instant>,
    pub(in crate::agent_console) previous_unclean: bool,
}

type Loaded = (HashMap<String, SyncedFile>, HashSet<String>, u64);

/// What `open` found: the versions and marks to trust, and, for a state it could not take
/// whole, why its files could not be removed (the caller turns the state off).
pub(in crate::agent_console) type Opened = (SyncStateFile, HashMap<String, SyncedFile>, HashSet<String>, Option<std::io::Error>);

/// A file that is not there: missing, or its folder is not a folder.
fn absent(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory)
}

impl SyncStateFile {
    /// The state beside `vars` for the Mac folder `host_dir`, and what it holds when it can be
    /// trusted (otherwise nothing: every decision is then the one without a state). A state not
    /// taken whole (either file unreadable, corrupt or for another folder) is removed here,
    /// before anything is sent: this session numbers its marks from 0, and a surviving older
    /// file with a higher number would outrank them at the next start.
    pub(in crate::agent_console) fn open(vars: &Path, host_dir: &Path) -> Opened {
        let side = |suffix: &str| {
            let mut name = vars.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        };
        let mut root = String::new();
        base64_encode_into(host_dir.as_os_str().as_bytes(), &mut root);
        // One list with the snapshot restore, which removes these files when it rolls the disk back.
        let [versions, marks] = bridgevm_hvf::snapshot_pair::SHARE_STATE_SUFFIXES;
        let mut file = Self {
            versions: side(versions),
            marks: Arc::new(MarksFile {
                state: Mutex::new(Marks {
                    path: side(marks),
                    root: root.clone(),
                    seq: 0,
                    names: HashSet::new(),
                }),
                write: Mutex::new(()),
                durable: AtomicU64::new(0),
                failed: AtomicBool::new(false),
                off: AtomicBool::new(false),
                #[cfg(test)]
                hold: AtomicBool::new(false),
                #[cfg(test)]
                hold_write: AtomicBool::new(false),
                #[cfg(test)]
                parked: AtomicBool::new(false),
            }),
            root,
            base: None,
            writer: None,
            thread: None,
            removed_unsynced: false,
            last_flush: None,
            previous_unclean: true,
        };
        let mut unremoved = None;
        let (files, marks, seq) = file.load().unwrap_or_else(|| {
            println!("BVAGENT SHARE sync-state ignored");
            match file.invalidate() {
                Ok(true) => {}
                Ok(false) => unremoved = Some(std::io::Error::other("state file busy")),
                Err(error) => unremoved = Some(error),
            }
            Default::default()
        });
        {
            let mut held = file.lock();
            held.seq = seq;
            held.names = marks.clone();
        }
        file.marks.durable.store(seq, Ordering::Release);
        file.base = Some(files.clone());
        file.previous_unclean = std::fs::read_to_string(&file.versions).ok()
            .is_none_or(|text| !text.lines().any(|line| line == "session clean"));
        // Persist uncertainty before any share mutation, including when old state was loaded.
        if let Err(error) = file.write_session(false, &files) {
            file.previous_unclean = true;
            unremoved = Some(error);
        }
        (file, files, marks, unremoved)
    }

    fn write_session(&self, clean: bool, files: &HashMap<String, SyncedFile>) -> std::io::Result<()> {
        let held = self.lock();
        let mut text = format!("bridgevm-share-sync 2\nroot {}\nseq {}\nsession {}\n", self.root, held.seq, if clean { "clean" } else { "unclean" });
        push_marks(&mut text, held.names.iter());
        for (name, file) in files {
            let mut b64 = String::new(); base64_encode_into(name.as_bytes(), &mut b64);
            let prev = file.prev.map_or("- -".to_string(), |v| format!("{} {}", v.size, v.mtime_ms));
            let _ = writeln!(text, "file {b64} {} {} {prev}", file.last.size, file.last.mtime_ms);
        }
        let file = write_replace(&self.versions, text.as_bytes())?;
        file.sync_all()?;
        std::fs::File::open(self.versions.parent().unwrap())?.sync_all()
    }

    /// Only observed PSCI SYSTEM_OFF calls this, after the service writer has ended.
    pub(in crate::agent_console) fn finish_clean(&mut self, engine: &mut ShareSync) {
        self.finish(engine);
        if self.marks.failed.load(Ordering::Acquire) { return; }
        if let Some((files, _, _)) = self.load() {
            if let Err(error) = self.write_session(true, &files) {
                println!("BVAGENT SHARE clean-state not-saved {error}");
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Marks> {
        self.marks.lock()
    }

    /// The marks writer, for a thread that publishes a fetched copy.
    pub(in crate::agent_console) fn marks(&self) -> SharedMarks {
        Arc::clone(&self.marks)
    }

    fn load(&self) -> Option<Loaded> {
        let read = |path: &Path| match std::fs::read_to_string(path) {
            Ok(text) => Some(Some(text)),
            Err(error) if absent(&error) => Some(None),
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

    /// Mark `names` on disk here (one write) before a fetched copy is published on this thread.
    pub(in crate::agent_console) fn mark<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> std::io::Result<()> {
        mark(&self.marks, names)
    }

    /// Ask for a PUT's marks, written by the state thread: the number the PUT waits for, or None
    /// when they are on disk already.
    pub(in crate::agent_console) fn ask_marks<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) -> Option<u64> {
        let seq = self.marks.ask(names);
        if self.marks.durable(seq) == Some(true) {
            return None;
        }
        let sent = self.writer().is_some_and(|writer| writer.send(ToWriter::Marks).is_ok());
        if !sent {
            self.marks.failed.store(true, Ordering::Release);
        }
        Some(seq)
    }

    /// Number `seq` is on disk (Some(true)), not yet (Some(false)), or never will be (None: a
    /// write failed or the state thread is gone).
    pub(in crate::agent_console) fn marks_durable(&self, seq: u64) -> Option<bool> {
        let durable = self.marks.durable(seq)?;
        if !durable && self.thread.as_ref().is_none_or(std::thread::JoinHandle::is_finished) {
            return None;
        }
        Some(durable)
    }

    /// Tests: wait until every mark asked is on disk (or failed), unless writes are held.
    #[cfg(test)]
    pub(in crate::agent_console) fn wait_marks(&self) {
        let seq = self.lock().seq;
        let until = Instant::now() + Duration::from_secs(5);
        while !self.marks.hold.load(Ordering::Acquire) && self.marks_durable(seq) == Some(false) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Settled marks leave, and the versions changed since the last flush go to the writer, at
    /// most once a second. False when a mark could not be written: the caller turns the state off.
    pub(in crate::agent_console) fn flush(&mut self, engine: &mut ShareSync, now: Instant) -> bool {
        if self.marks.failed.load(Ordering::Acquire) {
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
        // A copy from Windows its own thread still publishes marks nothing now: the next session
        // numbers its own marks in these files, and a late write here would replace them. A
        // write already past its check finishes first: its lock is taken (and let go) here.
        self.marks.off.store(true, Ordering::Release);
        drop(self.marks.write.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
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
            let (versions, marks, root) = (self.versions.clone(), Arc::clone(&self.marks), self.root.clone());
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

    /// The state is off for this session: no thread writes it any more. The state thread ends
    /// once it has seen what is queued (writing none of it); `invalidate` removes the files.
    pub(in crate::agent_console) fn turn_off(&mut self) {
        self.marks.off.store(true, Ordering::Release);
        self.writer = None;
    }

    /// Both files removed (missing counts) and, when one was, their folder synced, so no later
    /// session trusts them: Ok(true). Ok(false) while a write holds the lock (never waited for
    /// here: try on a later tick); an error when they cannot be removed.
    pub(in crate::agent_console) fn invalidate(&mut self) -> std::io::Result<bool> {
        let marks_path = self.lock().path.clone();
        let _one = match self.marks.write.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Ok(false),
        };
        for path in [&self.versions, &marks_path] {
            match std::fs::remove_file(path) {
                Ok(()) => self.removed_unsynced = true,
                Err(error) if absent(&error) => {}
                Err(error) => return Err(error),
            }
        }
        if let Some(dir) = self.versions.parent().filter(|_| self.removed_unsynced) {
            std::fs::File::open(dir)?.sync_all()?;
        }
        self.removed_unsynced = false;
        Ok(true)
    }

    /// Tests: the state thread has ended (after `turn_off`).
    #[cfg(test)]
    pub(in crate::agent_console) fn join_writer(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Tests hold the state thread before each write (bounded, so a failing test ends instead of
/// waiting here for good).
#[cfg(test)]
fn test_hold(marks: &MarksFile) {
    for _ in 0..5000 {
        if !marks.hold.load(Ordering::Acquire) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn run(versions: &Path, marks: &MarksFile, root: &str, mut files: HashMap<String, SyncedFile>, inbox: &mpsc::Receiver<ToWriter>) {
    let mut warned = false;
    while let Ok(message) = inbox.recv() {
        let (changes, marked, seq) = match message {
            ToWriter::Flush { changes, marks, seq } => (changes, marks, seq),
            ToWriter::Marks => {
                #[cfg(test)]
                test_hold(marks);
                // A failure sets `failed`; the service thread then turns the state off.
                let _ = marks.write_through();
                continue;
            }
        };
        for (name, file) in changes {
            match file {
                Some(file) => files.insert(name, file),
                None => files.remove(&name),
            };
        }
        let mut text = format!("bridgevm-share-sync 2\nroot {root}\nseq {seq}\nsession unclean\n");
        push_marks(&mut text, marked.iter());
        let mut b64 = String::new();
        for (name, file) in &files {
            b64.clear();
            base64_encode_into(name.as_bytes(), &mut b64);
            let prev = file.prev.map_or("- -".to_string(), |v| format!("{} {}", v.size, v.mtime_ms));
            let _ = writeln!(text, "file {b64} {} {} {prev}", file.last.size, file.last.mtime_ms);
        }
        #[cfg(test)]
        test_hold(marks);
        // Under the write lock: once the state is off (and its files removed) nothing re-creates it.
        let _one = marks.write.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if marks.off.load(Ordering::Acquire) {
            continue;
        }
        if let Err(error) = write_replace(versions, text.as_bytes()) {
            marks.failed.store(true, Ordering::Release);
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
    let found = lines.next()?;
    if (found != header && !(header == VERSIONS_HEADER && found == "bridgevm-share-sync 2")) || lines.next()?.strip_prefix("root ")? != root {
        return None;
    }
    let seq = lines.next()?.strip_prefix("seq ")?.parse().ok()?;
    let name = |b64: &str| String::from_utf8(base64_decode(b64).ok()?).ok();
    let (mut files, mut marks) = (HashMap::new(), HashSet::new());
    for line in lines {
        let mut it = line.split(' ');
        match it.next()? {
            "session" => {},
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
