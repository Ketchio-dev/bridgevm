//! A full Mac disk pauses the VM, as QEMU's default `werror=enospc` does
//! (decision 2026-11-09). A disk write or sync that fails with ENOSPC or
//! EDQUOT does not complete: Windows would take the error as lost data it
//! meant to keep. The controller holds the command instead, with its
//! completion slot, so the queue depth bounds what it holds, and raises
//! [`NvmeController::storage_full_flag`]. vCPU0 polls that before each run
//! and pauses the VM. On RESUME, before any vCPU runs,
//! [`NvmeController::retry_held`] runs the held commands again in order,
//! inline under the platform lock, and completes them as usual; the first one
//! that still finds the disk full stays held with those after it, and the VM
//! stays paused. The guest's buffers and PRP lists stay valid meanwhile: it
//! has seen no completion. Other host errors complete with today's statuses.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Internal status of a command held for host space, never posted: an NVMe
/// status has 15 bits (DNR is bit 14), so bit 15 marks it and the low bits
/// carry the errno.
const HELD: u16 = 0x8000;

/// The status for a failed disk write or sync: held when the host volume is
/// full or over quota, else `otherwise`, as before.
pub(crate) fn host_failure(error: &io::Error, otherwise: u16) -> u16 {
    match error.raw_os_error() {
        Some(errno @ (libc::ENOSPC | libc::EDQUOT)) => HELD | errno as u16,
        _ => otherwise,
    }
}

fn held_errno(status: u16) -> Option<i32> {
    (status & HELD != 0).then_some(i32::from(status & !HELD))
}

/// A WRITE or FLUSH waiting for host space: the entry holds its namespace,
/// LBA range, PRPs and FUA bit.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeldCommand {
    sqid: u16,
    cqid: u16,
    cmd: SubmissionEntry,
    errno: i32,
}

/// Why the VM paused: the host volume that holds the disk file is full.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskFull {
    pub errno: i32,
    /// Bytes free to an unprivileged writer on that volume.
    pub free_bytes: u64,
    /// Its mount point (empty when the host cannot say).
    pub volume: PathBuf,
}

impl NvmeController {
    /// Hold `cmd` instead of completing it when `status` says the disk was
    /// full. True when held.
    pub(crate) fn hold_if_full(
        &mut self,
        sqid: u16,
        cqid: u16,
        cmd: &SubmissionEntry,
        status: u16,
    ) -> bool {
        let Some(errno) = held_errno(status) else {
            return false;
        };
        self.hold_completion_slot(cqid);
        self.held.push(HeldCommand {
            sqid,
            cqid,
            cmd: *cmd,
            errno,
        });
        self.storage_full.store(true, Ordering::Release);
        // vCPU0 leaves hv_vcpu_run and pauses before it runs again.
        if let Some(hook) = FLUSH_DONE_HOOK.get() {
            hook();
        }
        true
    }

    /// Raised while commands wait for host space; vCPU0 polls it without the
    /// platform lock.
    pub fn storage_full_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.storage_full)
    }

    /// The volume the first held command waits on, while any is held.
    pub fn disk_full(&self) -> Option<DiskFull> {
        let held = self.held.first()?;
        let backend = self.backend_for_nsid(held.cmd.nsid).unwrap_or(&self.disk);
        let (free_bytes, volume) = match backend {
            DiskBackend::RawFile(disk) => volume_of(&disk.file).unwrap_or_default(),
            DiskBackend::Memory(_) => Default::default(),
        };
        Some(DiskFull {
            errno: held.errno,
            free_bytes,
            volume,
        })
    }

    /// RESUME: wait for the transfers and syncs in flight (a late failure is
    /// held too), then run the held commands again in order and post their
    /// completions. False while the disk is still full: the rest stay held.
    pub fn retry_held(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        completions: &mut Vec<NvmeCompletionEvent>,
    ) -> bool {
        if self.held.is_empty() {
            return true;
        }
        self.complete_flushes(mem, completions, true);
        let held = std::mem::take(&mut self.held);
        for (
            at,
            &HeldCommand {
                sqid, cqid, cmd, ..
            },
        ) in held.iter().enumerate()
        {
            let status = self.execute_io(&cmd, mem).status;
            if let Some(errno) = held_errno(status) {
                self.held = held[at..].to_vec();
                self.held[0].errno = errno;
                return false;
            }
            if let Some(slot) = self.flush_reserved.get_mut(usize::from(cqid)) {
                *slot = slot.saturating_sub(1);
            }
            completions.extend(self.complete_taken(sqid, cqid, &cmd, status, mem));
        }
        self.storage_full.store(false, Ordering::Release);
        true
    }

    /// A reset, restore or SQ deletion drops the held commands of the SQs
    /// `dropped` picks, with their completion slots.
    pub(crate) fn drop_held(&mut self, dropped: impl Fn(u16) -> bool) {
        let reserved = &mut self.flush_reserved;
        self.held.retain(|held| {
            if !dropped(held.sqid) {
                return true;
            }
            if let Some(slot) = reserved.get_mut(usize::from(held.cqid)) {
                *slot = slot.saturating_sub(1);
            }
            false
        });
        if self.held.is_empty() {
            self.storage_full.store(false, Ordering::Release);
        }
    }
}

/// The free bytes and mount point of the volume holding `file`.
#[cfg(target_os = "macos")]
fn volume_of(file: &std::fs::File) -> Option<(u64, PathBuf)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    // statfs, not statvfs: macOS statvfs counts blocks in 32 bits.
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: a live descriptor and writable statfs storage.
    if unsafe { libc::fstatfs(file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: fstatfs succeeded and filled the storage.
    let stat = unsafe { stat.assume_init() };
    // SAFETY: the kernel NUL-terminates f_mntonname.
    let mount = unsafe { std::ffi::CStr::from_ptr(stat.f_mntonname.as_ptr()) };
    Some((
        stat.f_bavail.saturating_mul(u64::from(stat.f_bsize)),
        PathBuf::from(std::ffi::OsStr::from_bytes(mount.to_bytes())),
    ))
}

#[cfg(not(target_os = "macos"))]
fn volume_of(_file: &std::fs::File) -> Option<(u64, PathBuf)> {
    None
}

/// Host write and sync errors the tests inject, by file (device, inode), so
/// tests on their own temporary files run in parallel.
#[cfg(test)]
pub(crate) mod faults {
    use std::io;
    use std::os::fd::RawFd;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;
    use std::sync::Mutex;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Op {
        Write,
        Sync,
    }

    type Fault = ((u64, u64), Op, i32);

    static FAULTS: Mutex<Vec<Fault>> = Mutex::new(Vec::new());

    fn faults() -> std::sync::MutexGuard<'static, Vec<Fault>> {
        FAULTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Fail every `op` on the file at `path` with `errno`, or no longer.
    pub(crate) fn set(path: &Path, op: Op, errno: Option<i32>) {
        let meta = std::fs::metadata(path).expect("a test file");
        let id = (meta.dev(), meta.ino());
        let mut faults = faults();
        faults.retain(|&(file, kind, _)| (file, kind) != (id, op));
        faults.extend(errno.map(|errno| (id, op, errno)));
    }

    /// Before the host call: the injected error for `op` on `fd`, if any.
    pub(crate) fn check(fd: RawFd, op: Op) -> io::Result<()> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: a live descriptor and writable stat storage.
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
            return Ok(());
        }
        // SAFETY: fstat succeeded and filled the storage.
        let stat = unsafe { stat.assume_init() };
        let id = (stat.st_dev as u64, stat.st_ino);
        match faults()
            .iter()
            .find(|&&(file, kind, _)| (file, kind) == (id, op))
        {
            Some(&(_, _, errno)) => Err(io::Error::from_raw_os_error(errno)),
            None => Ok(()),
        }
    }
}
