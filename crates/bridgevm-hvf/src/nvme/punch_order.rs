//! The Deallocates in flight on the vCPUs' NVMe workers, and the WRITEs that
//! must not run before them (dataset_management.rs, "Ordering").
//!
//! The controller lists each such punch, under the platform lock, from its
//! fetch until the first drain after it returned. While the list is empty,
//! which is all the time but a punch's few ms, a WRITE's fetch checks only
//! that: no ticket, atomic or allocation. Otherwise a WRITE whose bytes
//! overlap the ranges of a listed, unfinished punch of its namespace waits
//! for it: on its worker, without the lock, or inline under the lock when it
//! cannot be deferred. A punch never waits for anything, so nothing
//! deadlocks.

use std::sync::{Arc, Condvar, Mutex, PoisonError};

use super::flush_worker::lock;
use super::*;
use crate::fwcfg::GuestMemoryMut;

/// One Deallocate punched without the platform lock.
pub(crate) struct Punch {
    nsid: u32,
    /// The command's byte ranges, a superset of the host blocks punched.
    pub(super) ranges: Box<[(u64, u64)]>,
    /// Set once its `F_PUNCHHOLE` calls returned, or its job went unrun.
    done: Mutex<bool>,
    changed: Condvar,
    /// WRITEs that found it unfinished and waited.
    #[cfg(test)]
    pub(super) waiters: std::sync::atomic::AtomicUsize,
}

impl std::fmt::Debug for Punch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Punch")
            .field("nsid", &self.nsid)
            .field("ranges", &self.ranges.len())
            .field("done", &self.is_done())
            .finish()
    }
}

impl Punch {
    pub(super) fn new(nsid: u32, ranges: Vec<(u64, u64)>) -> Self {
        Self {
            nsid,
            ranges: ranges.into(),
            done: Mutex::new(false),
            changed: Condvar::new(),
            #[cfg(test)]
            waiters: Default::default(),
        }
    }

    pub(super) fn finish(&self) {
        *lock(&self.done) = true;
        self.changed.notify_all();
    }

    pub(super) fn is_done(&self) -> bool {
        *lock(&self.done)
    }

    /// Block until the punch has returned.
    pub(super) fn wait(&self) {
        let mut done = lock(&self.done);
        #[cfg(test)]
        if !*done {
            self.waiters
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        while !*done {
            done = self
                .changed
                .wait(done)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// True when bytes `start..end` of `nsid` share a byte with its ranges.
    fn overlaps(&self, nsid: u32, start: u64, end: u64) -> bool {
        self.nsid == nsid
            && self
                .ranges
                .iter()
                .any(|&(from, to)| from < end && start < to)
    }
}

impl NvmeController {
    /// Slow path, only while punches are listed: the unfinished ones a WRITE
    /// of bytes `start..end` of `nsid`, fetched now, must wait for.
    pub(super) fn punches_under(&self, nsid: u32, start: u64, end: u64) -> Vec<Arc<Punch>> {
        self.punches
            .iter()
            .filter(|punch| punch.overlaps(nsid, start, end) && !punch.is_done())
            .cloned()
            .collect()
    }

    /// An inline WRITE of bytes `start..end` of `nsid` (one that could not be
    /// deferred) waits for the punches under it. This is a platform-lock hold
    /// for the punch's length, but only when such a WRITE overlaps a punch
    /// still running, which a guest does not do with valid PRPs. A punch this
    /// doorbell queued and its vCPU has not taken would never run meanwhile,
    /// so the untaken jobs run here first, in `mem`.
    pub(super) fn wait_for_punches(
        &mut self,
        nsid: u32,
        start: u64,
        end: u64,
        mem: &dyn GuestMemoryMut,
    ) {
        let under = self.punches_under(nsid, start, end);
        if under.is_empty() {
            return;
        }
        // SAFETY: they run here and now, in the caller's view of guest RAM,
        // which stays mapped for this call.
        if let Some(io) = unsafe { self.take_io() } {
            io.run(mem);
        }
        for punch in under {
            punch.wait();
        }
    }

    /// A drain forgets the punches that returned.
    pub(super) fn forget_finished_punches(&mut self) {
        self.punches.retain(|punch| !punch.is_done());
    }
}
