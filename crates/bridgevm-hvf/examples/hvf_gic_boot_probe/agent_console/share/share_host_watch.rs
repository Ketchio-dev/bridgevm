//! FSEvents on the Mac folder, so an idle share is not walked every poll.
//!
//! A walk of 10,000 files cost vCPU 0's thread, which runs this service,
//! about 24 ms every 2 s poll. A complete walk's file list is planned from
//! again while FSEvents has reported nothing below the root since the walk
//! began, the service has not written or deleted a file there itself, and the
//! list is under a minute old. FSEvents names the path a change was made
//! through, so an edit through a hard link outside the folder waits for that
//! minute. Network volumes report only this Mac's changes and are always
//! walked, as is any folder where the stream does not start.

use super::*;
use std::ffi::{c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// The longest a walk is planned from without walking again.
pub(in crate::agent_console) const HOST_WALK_MAX_AGE: Duration = Duration::from_secs(60);

pub(in crate::agent_console) enum HostWatch {
    /// Every poll walks: tests, and folders FSEvents cannot cover.
    Off,
    /// Started before the next walk.
    Pending,
    On {
        changes: Arc<AtomicU64>,
        _stream: Option<FsEventStream>,
    },
}

impl HostWatch {
    /// A watch whose changes a test reports.
    #[cfg(test)]
    pub(in crate::agent_console) fn manual(changes: Arc<AtomicU64>) -> Self {
        Self::On { changes, _stream: None }
    }

    fn changes(&self) -> Option<u64> {
        match self {
            Self::On { changes, .. } => Some(changes.load(Ordering::Acquire)),
            _ => None,
        }
    }
}

/// A complete walk: its files, the names it could not read, and the watch's
/// count, the time and the wall clock when it began; its serial (share_idle.rs).
pub(in crate::agent_console) struct HostWalk {
    serial: u64,
    files: Vec<HostFile>,
    unseen: Vec<String>,
    changes: u64,
    at: Instant,
    wall: std::time::SystemTime,
}

impl ShareState {
    /// The last walk's serial, when nothing can have changed since it began.
    pub(in crate::agent_console) fn kept_host_walk(&self, now: Instant) -> Option<u64> {
        let current = self.host_watch.changes()?;
        let walk = self.host_walk.as_ref()?;
        (walk.changes == current && now.duration_since(walk.at) < HOST_WALK_MAX_AGE).then_some(walk.serial)
    }

    /// The kept walk's files in `host_scan_work`, and its unreadable names (kept_host_walk).
    pub(in crate::agent_console) fn reuse_host_walk(&mut self) -> Vec<String> {
        let Some(walk) = self.host_walk.as_ref() else {
            return Vec::new();
        };
        self.host_scan_work.clear();
        self.host_scan_work.extend(walk.files.iter().cloned());
        self.host_actions_scanned = walk.wall;
        walk.unseen.clone()
    }

    /// The serial of the walk the next plan comes from, when it is kept.
    pub(in crate::agent_console) fn host_walk_serial(&self) -> Option<u64> {
        self.host_walk.as_ref().map(|walk| walk.serial)
    }

    /// The kept walk's files and unreadable names.
    pub(in crate::agent_console) fn kept_host_walk_files(&self) -> Option<(&[HostFile], &[String])> {
        self.host_walk.as_ref().map(|walk| (walk.files.as_slice(), walk.unseen.as_slice()))
    }

    /// Before a walk: start a pending watch and note its count.
    pub(in crate::agent_console) fn begin_host_walk(&mut self) -> Option<u64> {
        self.host_walk = None;
        if matches!(self.host_watch, HostWatch::Pending) {
            self.host_watch = FsEventStream::start(&self.host_dir).map_or(HostWatch::Off, |(stream, changes)| {
                HostWatch::On { changes, _stream: Some(stream) }
            });
        }
        self.host_watch.changes()
    }

    /// After a complete walk that began at `changes`, keep its result.
    pub(in crate::agent_console) fn keep_host_walk(&mut self, changes: Option<u64>, unseen: &[String], now: Instant) {
        if let Some(changes) = changes {
            self.host_walk = Some(HostWalk {
                serial: self.new_idle_input(),
                files: self.host_scan_work.clone(),
                unseen: unseen.to_vec(),
                changes,
                at: now,
                wall: self.host_actions_scanned,
            });
        }
    }

    /// The service writes or deletes a Mac file: the next poll walks.
    pub(in crate::agent_console) fn forget_host_walk(&mut self) {
        self.host_walk = None;
    }
}

/// A started FSEvents stream on its own dispatch queue; each delivery adds
/// its event count to the shared counter.
pub(in crate::agent_console) struct FsEventStream {
    stream: *mut c_void,
    queue: *mut c_void,
}

// SAFETY: the stream is only stopped, invalidated and released (FSEvents
// allows that from any thread); its callback touches only the atomic counter.
unsafe impl Send for FsEventStream {}

impl FsEventStream {
    pub(in crate::agent_console) fn start(root: &Path) -> Option<(Self, Arc<AtomicU64>)> {
        let root = std::fs::canonicalize(root).ok()?;
        if !is_local_volume(&root) {
            return None;
        }
        let path = CString::new(root.as_os_str().as_bytes()).ok()?;
        let changes = Arc::new(AtomicU64::new(0));
        let context = FsEventStreamContext {
            version: 0,
            info: Arc::as_ptr(&changes) as *mut c_void,
            retain: Some(retain_changes),
            release: Some(release_changes),
            copy_description: std::ptr::null(),
        };
        // SAFETY: every CF object is checked and released once; the stream
        // retains the counter through the context callbacks, so the callback's
        // pointer outlives any delivery; stop, invalidate and release happen
        // once, in Drop or on a failed start.
        unsafe {
            let text = CFStringCreateWithBytes(
                std::ptr::null(),
                path.as_ptr().cast(),
                path.as_bytes().len() as isize,
                UTF8,
                0,
            );
            if text.is_null() {
                return None;
            }
            let paths = CFArrayCreate(std::ptr::null(), &text, 1, &kCFTypeArrayCallBacks);
            CFRelease(text);
            if paths.is_null() {
                return None;
            }
            let stream = FSEventStreamCreate(
                std::ptr::null(),
                on_events,
                &context,
                paths,
                SINCE_NOW,
                LATENCY_S,
                FLAG_NO_DEFER | FLAG_WATCH_ROOT,
            );
            CFRelease(paths);
            if stream.is_null() {
                return None;
            }
            let queue = dispatch_queue_create(c"bridgevm-share-watch".as_ptr(), std::ptr::null());
            if queue.is_null() {
                FSEventStreamRelease(stream);
                return None;
            }
            FSEventStreamSetDispatchQueue(stream, queue);
            let watch = Self { stream, queue };
            if FSEventStreamStart(stream) == 0 {
                return None;
            }
            Some((watch, changes))
        }
    }
}

impl Drop for FsEventStream {
    fn drop(&mut self) {
        // SAFETY: a stream created, scheduled and (maybe) started by `start`.
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            FSEventStreamRelease(self.stream);
            dispatch_release(self.queue);
        }
    }
}

fn is_local_volume(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: a NUL-terminated path and writable storage, read only on success.
    unsafe { libc::statfs(path.as_ptr(), fs.as_mut_ptr()) == 0 && fs.assume_init().f_flags & libc::MNT_LOCAL as u32 != 0 }
}

extern "C" fn on_events(
    _stream: *const c_void,
    info: *mut c_void,
    count: usize,
    _paths: *mut c_void,
    _flags: *const u32,
    _ids: *const u64,
) {
    // SAFETY: `info` is the counter the stream retained (`retain_changes`).
    let changes = unsafe { &*(info as *const AtomicU64) };
    changes.fetch_add(count.max(1) as u64, Ordering::Release);
}

extern "C" fn retain_changes(info: *const c_void) -> *const c_void {
    // SAFETY: `info` came from Arc::as_ptr on a live counter.
    unsafe { Arc::increment_strong_count(info as *const AtomicU64) };
    info
}

extern "C" fn release_changes(info: *const c_void) {
    // SAFETY: balances one retain_changes.
    unsafe { Arc::decrement_strong_count(info as *const AtomicU64) };
}

const UTF8: u32 = 0x0800_0100;
const SINCE_NOW: u64 = u64::MAX;
const LATENCY_S: f64 = 0.05;
const FLAG_NO_DEFER: u32 = 0x02;
const FLAG_WATCH_ROOT: u32 = 0x04;

#[repr(C)]
struct FsEventStreamContext {
    version: isize,
    info: *mut c_void,
    retain: Option<extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<extern "C" fn(*const c_void)>,
    copy_description: *const c_void,
}

type FsEventsCallback = extern "C" fn(*const c_void, *mut c_void, usize, *mut c_void, *const u32, *const u64);

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn FSEventStreamCreate(
        allocator: *const c_void,
        callback: FsEventsCallback,
        context: *const FsEventStreamContext,
        paths: *const c_void,
        since_when: u64,
        latency: f64,
        flags: u32,
    ) -> *mut c_void;
    fn FSEventStreamSetDispatchQueue(stream: *mut c_void, queue: *mut c_void);
    fn FSEventStreamStart(stream: *mut c_void) -> u8;
    fn FSEventStreamStop(stream: *mut c_void);
    fn FSEventStreamInvalidate(stream: *mut c_void);
    fn FSEventStreamRelease(stream: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFTypeArrayCallBacks: [usize; 5];
    fn CFStringCreateWithBytes(
        alloc: *const c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external: u8,
    ) -> *const c_void;
    fn CFArrayCreate(
        alloc: *const c_void,
        values: *const *const c_void,
        count: isize,
        callbacks: *const [usize; 5],
    ) -> *const c_void;
    fn CFRelease(object: *const c_void);
}

extern "C" {
    fn dispatch_queue_create(label: *const std::ffi::c_char, attr: *const c_void) -> *mut c_void;
    fn dispatch_release(object: *mut c_void);
}

#[cfg(test)]
#[path = "share_host_watch_tests.rs"]
mod tests;
