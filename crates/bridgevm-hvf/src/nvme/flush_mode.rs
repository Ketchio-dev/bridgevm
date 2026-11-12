//! What a guest NVMe FLUSH or FUA write costs on the host, from
//! `BRIDGEVM_NVME_FLUSH`:
//! - default, ordered: the flush worker answers with `F_BARRIERFSYNC`, which
//!   reaches the drive in order, and with `F_FULLFSYNC` instead once that
//!   disk's last full sync is a second old; it also runs the full sync a
//!   second after a disk's first barrier-only flush. Each disk keeps its own
//!   debt, since a full sync of one file does not make another durable. A
//!   sudden host power loss leaves the guest file system consistent and loses
//!   at most about the last second of acknowledged writes. A flush run inline
//!   (no worker: checkpoints, `BRIDGEVM_NVME_ASYNC_FLUSH=0`; or disabling the
//!   write cache) has no timer to follow a barrier, so it is a full sync.
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
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How stale a disk's last full sync may get in the ordered mode.
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

/// The guest-visible flush mode, for the boot log; `inline` without the
/// flush worker.
pub fn host_sync_label(inline: bool) -> &'static str {
    match mode() {
        FlushMode::Ordered if inline => "durable (F_FULLFSYNC: inline flushes have no timer)",
        FlushMode::Ordered => "ordered (F_BARRIERFSYNC, F_FULLFSYNC within 1 s)",
        FlushMode::Durable => "durable (F_FULLFSYNC)",
        FlushMode::Fast => "fast (fsync)",
    }
}

/// Sync on the controller thread: a guest FLUSH or FUA write without the flush
/// worker, disabling the write cache, and (`durable`) a clean shutdown or VM
/// exit. No timer would follow a barrier here with the promised full sync, so
/// only the fast mode skips it.
pub(crate) fn sync_inline(file: &File, durable: bool) -> io::Result<()> {
    #[cfg(test)]
    super::disk_full::faults::check(file.as_raw_fd(), super::disk_full::faults::Op::Sync)?;
    if mode() == FlushMode::Fast && !durable {
        fsync(file)
    } else {
        full_sync(file)
    }
}

/// A disk's handle on the flush worker and the full sync it still owes.
pub(crate) struct DiskSync {
    file: File,
    /// When this disk's last `F_FULLFSYNC` returned.
    last_full: Option<Instant>,
    /// An ordered flush of this disk was acknowledged after its last full sync.
    owed: bool,
    /// Only the first deferred failure and its recovery are logged.
    deferred_failed: bool,
}

impl DiskSync {
    pub(crate) fn new(file: File) -> Self {
        Self {
            file,
            last_full: None,
            owed: false,
            deferred_failed: false,
        }
    }

    /// Sync for guest FLUSH or FUA writes, in the configured mode.
    pub(crate) fn sync(&mut self, clock: &Clock) -> io::Result<()> {
        #[cfg(test)]
        super::disk_full::faults::check(self.file.as_raw_fd(), super::disk_full::faults::Op::Sync)?;
        match mode() {
            FlushMode::Ordered if barrier_suffices(self.last_full, clock.now()) => {
                barrier_sync(&self.file)?;
                self.owed = true;
                Ok(())
            }
            FlushMode::Ordered | FlushMode::Durable => self.full_sync(clock),
            FlushMode::Fast => fsync(&self.file),
        }
    }

    /// An acknowledged ordered flush still waits for its full sync.
    pub(crate) fn owed(&self) -> bool {
        self.owed
    }

    /// Run the full sync this disk owes, if any; a failure leaves it owed.
    pub(crate) fn settle(&mut self, clock: &Clock) {
        if self.owed {
            if let Err(error) = self.full_sync(clock) {
                if !self.deferred_failed {
                    crate::host_log::err(format_args!(
                        "nvme: deferred full sync failed: {error}; acknowledged writes are not durable, check the drive connection"
                    ));
                    crate::host_log::err("BVSTORAGE durability-failed");
                    self.deferred_failed = true;
                }
            }
        }
    }

    fn full_sync(&mut self, clock: &Clock) -> io::Result<()> {
        full_sync(&self.file)?;
        self.last_full = Some(clock.now());
        self.owed = false;
        if self.deferred_failed {
            crate::host_log::err(
                "nvme: deferred full sync recovered; acknowledged writes are durable",
            );
            self.deferred_failed = false;
        }
        Ok(())
    }
}

/// In the ordered mode a barrier suffices while the disk's last full sync is
/// under a second old.
fn barrier_suffices(last_full: Option<Instant>, now: Instant) -> bool {
    last_full.is_some_and(|at| now.saturating_duration_since(at) < DURABLE_WITHIN)
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

/// `F_FULLFSYNC` on macOS (Rust's `sync_data`), `fdatasync` elsewhere: the
/// data survives a power loss once this returns.
fn full_sync(file: &File) -> io::Result<()> {
    #[cfg(test)]
    super::disk_full::faults::check(file.as_raw_fd(), super::disk_full::faults::Op::Sync)?;
    file.sync_data()?;
    #[cfg(test)]
    record(file, SyncKind::Full);
    Ok(())
}

fn fsync(file: &File) -> io::Result<()> {
    // SAFETY: `file` owns a live descriptor for the whole call.
    if unsafe { libc::fsync(file.as_raw_fd()) } != 0 {
        return Err(io::Error::last_os_error());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_sync_failure_is_logged_once_and_recovery_is_logged() {
        const CHILD: &str = "BRIDGEVM_TEST_DEFERRED_SYNC_LOG";
        if std::env::var_os(CHILD).is_some() {
            let clock = Clock::default();
            let path = std::env::temp_dir().join(format!("bridgevm-sync-{}", std::process::id()));
            let mut disk = DiskSync::new(File::create(&path).unwrap());
            disk.sync(&clock).unwrap();
            disk.sync(&clock).unwrap();
            assert!(disk.owed());
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                Some(libc::EIO),
            );
            clock.advance(DURABLE_WITHIN);
            disk.settle(&clock);
            assert!(disk.owed());
            clock.advance(DURABLE_WITHIN);
            disk.settle(&clock);
            assert!(disk.owed());
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                None,
            );
            disk.settle(&clock);
            assert!(!disk.owed());
            disk.settle(&clock);
            disk.sync(&clock).unwrap();
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                Some(libc::EIO),
            );
            clock.advance(DURABLE_WITHIN);
            disk.settle(&clock);
            disk.settle(&clock);
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                None,
            );
            std::fs::remove_file(path).unwrap();
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "nvme::flush_mode::tests::deferred_sync_failure_is_logged_once_and_recovery_is_logged", "--nocapture"])
            .env(CHILD, "1")
            .output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let log = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            log.matches("nvme: deferred full sync failed:").count(),
            2,
            "{log}"
        );
        assert!(log.contains("os error"), "errno missing: {log}");
        assert_eq!(
            log.matches("BVSTORAGE durability-failed").count(),
            2,
            "{log}"
        );
        assert_eq!(
            log.matches("nvme: deferred full sync recovered").count(),
            1,
            "{log}"
        );
    }

    #[test]
    fn failed_final_sync_emits_one_final_save_event_before_failing() {
        const CHILD: &str = "BRIDGEVM_TEST_FINAL_SYNC_LOG";
        if std::env::var_os(CHILD).is_some() {
            let path =
                std::env::temp_dir().join(format!("bridgevm-final-sync-{}", std::process::id()));
            let file = File::create(&path).unwrap();
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                Some(libc::EIO),
            );
            let errors = vec![sync_inline(&file, true).unwrap_err().to_string(); 2];
            super::super::disk_full::faults::set(
                &path,
                super::super::disk_full::faults::Op::Sync,
                None,
            );
            std::fs::remove_file(path).unwrap();
            assert!(
                std::panic::catch_unwind(|| crate::host_log::assert_final_save(&errors)).is_err()
            );
            crate::host_log::assert_final_save(&[]);
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "nvme::flush_mode::tests::failed_final_sync_emits_one_final_save_event_before_failing", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let log = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            log.matches("BVSTORAGE final-save-failed").count(),
            1,
            "{log}"
        );
        assert!(log.contains("os error 5"), "{log}");
    }

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
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(!barrier_suffices(None, at(5)), "never synced");
        assert!(barrier_suffices(Some(at(0)), at(999)));
        assert!(!barrier_suffices(Some(at(0)), at(1000)));
        assert!(barrier_suffices(Some(at(1000)), at(1500)));
    }
}
