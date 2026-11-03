//! What a guest NVMe FLUSH or FUA write costs on the host, from
//! `BRIDGEVM_NVME_FLUSH`:
//! - default, ordered: `F_BARRIERFSYNC`, which reaches the drive in order, and
//!   `F_FULLFSYNC` instead once the last full sync is a second old (the flush
//!   worker also runs one a second after the last flush). A sudden host power
//!   loss leaves the guest file system consistent and loses at most about the
//!   last second of acknowledged writes.
//! - `durable` (the user's per-VM opt-in): `F_FULLFSYNC` for every flush, so
//!   no acknowledged write is lost to a power loss.
//! - `fast` (measurement only): plain `fsync`, QEMU's choice on macOS; writes
//!   are neither ordered nor durable across a power loss.
//!
//! A clean guest shutdown (NVMe CC.SHN) and VM exit always end with
//! `F_FULLFSYNC`. 4 KiB write + sync, p50, internal SSD: fsync 0.02-0.05 ms,
//! `F_BARRIERFSYNC` 0.20 ms, `F_FULLFSYNC` 4.0 ms; on a USB SSD
//! `F_BARRIERFSYNC` costs what `F_FULLFSYNC` does (1.7 ms).

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How stale the last full sync may get in the ordered mode.
pub(crate) const DURABLE_WITHIN: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlushMode {
    Ordered,
    Durable,
    Fast,
}

impl FlushMode {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("durable") => Self::Durable,
            Some("fast") => Self::Fast,
            _ => Self::Ordered,
        }
    }
}

pub(crate) fn mode() -> FlushMode {
    static MODE: OnceLock<FlushMode> = OnceLock::new();
    *MODE.get_or_init(|| FlushMode::parse(std::env::var("BRIDGEVM_NVME_FLUSH").ok().as_deref()))
}

/// The guest-visible flush mode, for the boot log.
pub fn host_sync_label() -> &'static str {
    match mode() {
        FlushMode::Ordered => "ordered (F_BARRIERFSYNC, F_FULLFSYNC within 1 s)",
        FlushMode::Durable => "durable (F_FULLFSYNC)",
        FlushMode::Fast => "fast (fsync)",
    }
}

fn clock() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// The flush worker's time. Tests move it forward instead of sleeping.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    #[cfg(test)]
    skew_nanos: AtomicU64,
}

impl Clock {
    #[cfg(not(test))]
    pub(crate) fn now(&self) -> Instant {
        Instant::now()
    }

    #[cfg(test)]
    pub(crate) fn now(&self) -> Instant {
        Instant::now() + Duration::from_nanos(self.skew_nanos.load(Ordering::Relaxed))
    }

    /// As if `by` had passed.
    #[cfg(test)]
    pub(crate) fn advance(&self, by: Duration) {
        let nanos = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
        self.skew_nanos.fetch_add(nanos, Ordering::Relaxed);
    }
}

/// Nanoseconds after `clock()` of the last `F_FULLFSYNC`, plus one (0: never).
static LAST_FULL_SYNC: AtomicU64 = AtomicU64::new(0);
/// An ordered flush was acknowledged after the last `F_FULLFSYNC`.
static FULL_SYNC_OWED: AtomicBool = AtomicBool::new(false);

fn full_sync_due(now: Duration, last_plus_one: u64) -> bool {
    last_plus_one == 0
        || now.saturating_sub(Duration::from_nanos(last_plus_one - 1)) >= DURABLE_WITHIN
}

/// Sync for a guest FLUSH or FUA write, in the configured mode.
pub(crate) fn sync(file: &File) -> io::Result<()> {
    sync_at(file, Instant::now())
}

/// [`sync`] as of `now`.
pub(crate) fn sync_at(file: &File, now: Instant) -> io::Result<()> {
    match mode() {
        FlushMode::Durable => sync_durable(file),
        FlushMode::Ordered
            if full_sync_due(
                now.saturating_duration_since(clock()),
                LAST_FULL_SYNC.load(Ordering::Relaxed),
            ) =>
        {
            sync_durable(file)
        }
        FlushMode::Ordered => {
            barrier_sync(file)?;
            FULL_SYNC_OWED.store(true, Ordering::Relaxed);
            Ok(())
        }
        FlushMode::Fast => {
            // SAFETY: `file` owns a live descriptor for the whole call.
            if unsafe { libc::fsync(file.as_raw_fd()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}

/// `F_BARRIERFSYNC`: the data reaches the drive in order with earlier writes.
#[cfg(target_os = "macos")]
fn barrier_sync(file: &File) -> io::Result<()> {
    // SAFETY: `file` owns a live descriptor; the command takes no argument.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    #[cfg(test)]
    record(file, SyncKind::Barrier);
    Ok(())
}

/// Other hosts (cross-compiled stubs) have no barrier: sync the data.
#[cfg(not(target_os = "macos"))]
fn barrier_sync(file: &File) -> io::Result<()> {
    file.sync_data()?;
    #[cfg(test)]
    record(file, SyncKind::Barrier);
    Ok(())
}

/// `F_FULLFSYNC` on macOS (Rust's `sync_data`), `fdatasync` elsewhere.
fn full_sync(file: &File) -> io::Result<()> {
    file.sync_data()?;
    #[cfg(test)]
    record(file, SyncKind::Full);
    Ok(())
}

/// A host sync the tests observe.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncKind {
    Barrier,
    Full,
}

/// Completed syncs by file (device, inode): the worker syncs duplicated handles.
#[cfg(test)]
static SYNCS: std::sync::Mutex<Vec<((u64, u64), SyncKind)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn record(file: &File, kind: SyncKind) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(meta) = file.metadata() {
        let mut syncs = SYNCS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        syncs.push(((meta.dev(), meta.ino()), kind));
    }
}

/// The syncs of the file at `path` so far, oldest first.
#[cfg(test)]
pub(crate) fn recorded_syncs(path: &std::path::Path) -> Vec<SyncKind> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).expect("synced file exists");
    let id = (meta.dev(), meta.ino());
    let syncs = SYNCS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    syncs
        .iter()
        .filter(|(file, _)| *file == id)
        .map(|(_, kind)| *kind)
        .collect()
}

/// `F_FULLFSYNC`: the data survives a power loss once this returns.
pub(crate) fn sync_durable(file: &File) -> io::Result<()> {
    FULL_SYNC_OWED.store(false, Ordering::Relaxed);
    if let Err(error) = full_sync(file) {
        FULL_SYNC_OWED.store(true, Ordering::Relaxed);
        return Err(error);
    }
    let now = u64::try_from(clock().elapsed().as_nanos()).unwrap_or(u64::MAX - 1);
    LAST_FULL_SYNC.store(now + 1, Ordering::Relaxed);
    Ok(())
}

/// An ordered flush was acknowledged after the last full sync.
pub(crate) fn full_sync_owed() -> bool {
    FULL_SYNC_OWED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_is_the_default_and_only_explicit_choices_change_it() {
        assert_eq!(FlushMode::parse(None), FlushMode::Ordered);
        assert_eq!(FlushMode::parse(Some("")), FlushMode::Ordered);
        assert_eq!(FlushMode::parse(Some("full")), FlushMode::Ordered);
        assert_eq!(FlushMode::parse(Some("durable")), FlushMode::Durable);
        assert_eq!(FlushMode::parse(Some("fast")), FlushMode::Fast);
    }

    #[test]
    fn a_full_sync_is_due_first_and_then_once_the_last_one_is_a_second_old() {
        let at = |ms: u64| Duration::from_millis(ms);
        let last = |ms: u64| u64::try_from(at(ms).as_nanos()).unwrap() + 1;
        assert!(full_sync_due(at(5), 0), "never synced");
        assert!(!full_sync_due(at(999), last(0)));
        assert!(full_sync_due(at(1000), last(0)));
        assert!(!full_sync_due(at(1500), last(1000)));
    }
}
