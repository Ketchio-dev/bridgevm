//! UEFI variables saved while the VM runs, not only at a clean stop.
//!
//! The guest's variable writes (a new boot entry after a Windows feature
//! update, a Secure Boot db or dbx update) change only the in-memory pflash
//! bank (pflash.rs). Before this thread they reached the Mac disk only in the
//! stop-time save (stop_media.rs), so a crash, a host power loss or a Force
//! Stop whose save ran out of time paired the old vars with a disk whose
//! writes were durable within a second, and lost every vars change since the
//! start.
//!
//! - Debounced: every 500 ms the thread reads the bank's change count. It
//!   saves once the count has stayed the same for a second, and never while
//!   the count still matches the last save, so a burst of flash writes costs
//!   one save and an idle guest none.
//! - The stop's own save: `RuntimeLease::persist` for the vars slot, with the
//!   policy and the selected (managed-pair) path the runtime owner captured
//!   before the VM was created. It replaces the file through a staged copy
//!   synced with `F_FULLFSYNC` and a rename, so the file always holds the old
//!   or the new complete bank. The owner's lease keeps snapshot creation and
//!   restore out for the whole run, so the selected path cannot change.
//! - Off the vCPUs and outside the platform lock: under the lock the thread
//!   only reads the count and copies the bank (about 3 ms). The write
//!   (measured for 64 MiB: about 20 ms on a Mac's internal SSD, 0.4-2 s on
//!   an external USB SSD) runs unlocked on this thread, so no vCPU waits.
//! - The copy has pages of its own, unmapped after the save: macOS malloc
//!   keeps a freed 64 MiB block resident (probe_setup.rs).
//! - A failed save keeps the VM running: one line on stderr (run.log) the
//!   first time, then a retry after the guest's next change or after 10 s.
//!   The probe stops this thread before the stop-time save, which runs
//!   either way, so the two never overlap.

use std::io;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bridgevm_hvf::snapshot_pair::managed::runtime::{RuntimeLease, RuntimeMediaSlot};

use crate::{lock_platform, HoldSite, VirtPlatform};

/// How long the bank stays unchanged before it is saved.
const QUIET: Duration = Duration::from_secs(1);
/// How often the thread reads the change count.
const POLL: Duration = Duration::from_millis(500);
/// When a failed save is tried again if the guest changes nothing more.
const RETRY: Duration = Duration::from_secs(10);

/// When to save, from the bank's change count.
#[derive(Default)]
pub(crate) struct VarsAutosave {
    /// The count the vars file holds; 0 is the bank as read from the file.
    saved: u64,
    /// The count the last tick read.
    seen: u64,
    /// When an unsaved count may be saved.
    due: Option<Instant>,
    failure_reported: bool,
}

impl VarsAutosave {
    /// One tick at `now`: saves the bank once its last change is `QUIET` old,
    /// or a failed save's retry is due. Returns whether the file was replaced.
    pub(crate) fn tick(
        &mut self,
        platform: &Arc<Mutex<VirtPlatform>>,
        owner: &Mutex<RuntimeLease>,
        now: Instant,
    ) -> bool {
        let (changes, copy) = {
            let platform = lock_platform(platform, None, 0, HoldSite::VarsAutosave);
            let changes = platform.flash_vars_changes();
            if changes != self.seen {
                self.seen = changes;
                self.due = Some(now + QUIET);
            }
            if changes == self.saved || self.due.is_some_and(|due| now < due) {
                return false;
            }
            (changes, BankCopy::of(platform.flash_vars_image()))
        };
        let saved = copy.and_then(|copy| {
            let mut owner = owner.lock().unwrap_or_else(PoisonError::into_inner);
            owner.persist(RuntimeMediaSlot::Vars, copy.bytes())
        });
        match saved {
            Ok(_) => {
                self.saved = changes;
                self.due = None;
                true
            }
            Err(error) => {
                if !std::mem::replace(&mut self.failure_reported, true) {
                    eprintln!(
                        "UEFI vars could not be saved while the VM runs; BridgeVM keeps \
                         retrying and saves them again at stop: {error}"
                    );
                }
                self.due = Some(now + RETRY);
                false
            }
        }
    }
}

/// One save's copy of the bank, in an anonymous mapping unmapped on drop.
struct BankCopy {
    pages: *mut u8,
    len: usize,
}

impl BankCopy {
    fn of(bank: &[u8]) -> io::Result<Self> {
        let (prot, flags) = (
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
        );
        // SAFETY: a new private anonymous mapping that nothing else refers to.
        let pages = unsafe { libc::mmap(std::ptr::null_mut(), bank.len(), prot, flags, -1, 0) };
        if pages == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let copy = Self {
            pages: pages.cast(),
            len: bank.len(),
        };
        // SAFETY: the mapping holds `len` writable bytes and does not overlap `bank`.
        unsafe { std::ptr::copy_nonoverlapping(bank.as_ptr(), copy.pages, copy.len) };
        Ok(copy)
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: `of` filled all `len` bytes; the mapping lives until drop.
        unsafe { std::slice::from_raw_parts(self.pages, self.len) }
    }
}

impl Drop for BankCopy {
    fn drop(&mut self) {
        // SAFETY: the mapping `of` made, unmapped once; no borrow outlives `self`.
        unsafe { libc::munmap(self.pages.cast(), self.len) };
    }
}

/// The saver thread: one per probe process, started beside the HDA tick
/// when the vars are written back.
pub(crate) struct VarsAutosaveThread {
    stop: Sender<()>,
    thread: JoinHandle<()>,
}

impl VarsAutosaveThread {
    /// None, with one line, if the thread cannot start: the VM then runs
    /// with the stop-time save only.
    pub(crate) fn start(
        platform: &Arc<Mutex<VirtPlatform>>,
        owner: &Arc<Mutex<RuntimeLease>>,
    ) -> Option<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let (platform, owner) = (Arc::clone(platform), Arc::clone(owner));
        let thread = thread::Builder::new()
            .name("uefi-vars-autosave".into())
            .spawn(move || {
                let mut autosave = VarsAutosave::default();
                while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(POLL) {
                    autosave.tick(&platform, &owner, Instant::now());
                }
            })
            .map_err(|error| eprintln!("UEFI vars are saved only at stop: {error}"))
            .ok()?;
        Some(Self { stop, thread })
    }
}

/// Ends the saver after any save in progress, then hands the runtime owner
/// to the stop-time save.
pub(crate) fn stop_before_final_save(
    saver: Option<VarsAutosaveThread>,
    owner: &Mutex<RuntimeLease>,
) -> MutexGuard<'_, RuntimeLease> {
    if let Some(VarsAutosaveThread { stop, thread }) = saver {
        drop(stop);
        // A saver that panicked saved what it could; the stop saves the rest.
        let _ = thread.join();
    }
    owner.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "vars_autosave_tests.rs"]
mod tests;
