//! Deallocate punches off the platform lock. Punching a 1 GiB file's ranges
//! inline held the lock for 24-37 ms in one DSM command, longer than a 20 ms
//! audio buffer, and every vCPU that needed the lock stalled behind it.
//!
//! With unlocked I/O on (live VMMs; tests and checkpointing keep it off), a
//! DSM on a write-back raw file checks its range list as before, records the
//! valid byte ranges as pending punches of that namespace and completes at
//! once. A background thread, started on the first such DSM, punches them in
//! pieces of at most [`PIECE_BYTES`], each holding the one mutex that guards
//! the pending ranges for the whole `F_PUNCHHOLE`.
//!
//! Deallocation is advisory (DLFEAT 0), so a range punched late, or never,
//! leaves the guest's data as valid as a punched one. What must never happen
//! is a punch of bytes the guest wrote after the DSM. Every WRITE fetched
//! from a submission queue takes its byte range out of the pending punches
//! in `process_sq`, before any path (inline, unlocked or FUA) runs it, under
//! the same mutex: it either waits for the piece being punched, which ends
//! before its data is written, or comes first, and no later piece touches
//! its bytes. Each piece is rounded inward to whole host blocks, so a block
//! that a WRITE covers only in part keeps all its bytes. When nothing is
//! pending the check costs one atomic load. A WRITE fetched before the DSM
//! and still in flight may land before or after a punch of the same bytes:
//! NVMe orders nothing in flight, so both results are valid.
//!
//! The mutex does not queue its waiters, so between pieces the thread lets a
//! waiting WRITE, DSM or settle point in first: each waits for the piece
//! being punched (or one that started just as it came), never for the whole
//! list. The final report's `nvme dsm:` record has the longest piece.
//!
//! Settling: a reset, a CC.EN 1 to 0 write or a snapshot restore drops the
//! pending punches once the piece being punched ends; a wait for the jobs in
//! flight (shutdown through CC.SHN, the VM's exit sync, an export, a
//! namespace change) first punches them all, so a clean shutdown still gives
//! the space back before its durable sync. A namespace change also lets the
//! thread and its files go, and each namespace's ranges are kept with the
//! file they came from, so none reaches a different file. Dropping the
//! controller stops and joins the thread.

use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Instant;

use super::dataset_management::{punch_ranges, DSM_STATS};
use super::flush_worker::lock;
use super::*;

/// The most one piece punches. A piece ends at a multiple of it, which is a
/// multiple of any host block size, so splitting a range never leaves a
/// whole block unpunched.
pub(super) const PIECE_BYTES: u64 = 8 << 20;

/// Pending ranges per namespace (about 1.5 MiB at most); past it new ranges
/// are dropped, which only leaves their blocks allocated.
pub(super) const MAX_RANGES: usize = 65_536;

/// Disjoint byte ranges, start to end, none touching another.
#[derive(Default)]
pub(super) struct PendingRanges(BTreeMap<u64, u64>);

impl PendingRanges {
    /// Add `start..end`, merging the ranges it overlaps or touches; `false`
    /// when it is dropped at the bound.
    pub(super) fn add(&mut self, mut start: u64, mut end: u64) -> bool {
        if start >= end {
            return true;
        }
        if let Some((&s, &e)) = self.0.range(..=start).next_back() {
            if e >= start {
                self.0.remove(&s);
                (start, end) = (s, end.max(e));
            }
        }
        while let Some((&s, &e)) = self.0.range(start..=end).next() {
            self.0.remove(&s);
            end = end.max(e);
        }
        if self.0.len() >= MAX_RANGES {
            return false;
        }
        self.0.insert(start, end);
        true
    }

    /// Take `start..end` out; returns the ranges dropped because a split
    /// would pass the bound.
    pub(super) fn remove(&mut self, start: u64, end: u64) -> u64 {
        let mut dropped = 0;
        if let Some((&s, &e)) = self.0.range(..start).next_back() {
            if e > start {
                self.0.insert(s, start);
                if e > end {
                    if self.0.len() < MAX_RANGES {
                        self.0.insert(end, e);
                    } else {
                        dropped += 1;
                    }
                }
            }
        }
        while let Some((&s, &e)) = self.0.range(start..end).next() {
            self.0.remove(&s);
            if e > end {
                self.0.insert(end, e);
            }
        }
        dropped
    }

    /// The next piece: the lowest range up to the next multiple of
    /// [`PIECE_BYTES`].
    pub(super) fn take_piece(&mut self) -> Option<(u64, u64)> {
        let (start, end) = self.0.pop_first()?;
        let cut = (start / PIECE_BYTES + 1).saturating_mul(PIECE_BYTES);
        if end <= cut {
            return Some((start, end));
        }
        self.0.insert(cut, end);
        Some((start, cut))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(super) fn ranges(&self) -> Vec<(u64, u64)> {
        self.0.iter().map(|(&s, &e)| (s, e)).collect()
    }
}

/// One namespace's pending punches and the file they belong to.
struct Namespace {
    file: Arc<File>,
    ranges: PendingRanges,
}

#[derive(Default)]
struct State {
    /// NSID 1 and 2.
    disks: [Option<Namespace>; 2],
    stop: bool,
    /// Tests hold the thread's pieces.
    #[cfg(test)]
    paused: bool,
}

impl State {
    fn any_pending(&self) -> bool {
        self.disks
            .iter()
            .flatten()
            .any(|disk| !disk.ranges.is_empty())
    }

    /// The thread has a piece to punch.
    fn ready(&self) -> bool {
        #[cfg(test)]
        if self.paused {
            return false;
        }
        self.any_pending()
    }

    /// Punch the next piece; `false` when nothing is pending. The caller
    /// holds the mutex throughout.
    fn run_piece(&mut self) -> bool {
        let Some(disk) = self
            .disks
            .iter_mut()
            .flatten()
            .find(|disk| !disk.ranges.is_empty())
        else {
            return false;
        };
        let Some(piece) = disk.ranges.take_piece() else {
            return false;
        };
        let started = Instant::now();
        let punched = punch_ranges(&disk.file, &[piece]);
        DSM_STATS.note_piece(punched, started);
        true
    }
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    /// Some range is pending or being punched. Stored only under `state`,
    /// and cleared only after the last piece's punch has returned.
    pending: AtomicBool,
    /// I/O-path callers waiting for `state`; the thread lets them in before
    /// its next piece.
    waiting: AtomicUsize,
    /// The thread's pieces so far; the `lock_first` calls, and the most
    /// pieces the thread finished while one of them waited.
    #[cfg(test)]
    pieces: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    first_locks: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    most_passed: std::sync::atomic::AtomicU64,
}

impl Shared {
    fn publish(&self, state: &State) {
        self.pending.store(state.any_pending(), Ordering::Release);
    }

    /// Lock `state` from the I/O path (a DSM, a WRITE, a settle point, all
    /// under the platform lock), ahead of the thread's next piece.
    fn lock_first(&self) -> MutexGuard<'_, State> {
        self.waiting.fetch_add(1, Ordering::AcqRel);
        #[cfg(test)]
        let seen = self.pieces.load(Ordering::Relaxed);
        let state = lock(&self.state);
        #[cfg(test)]
        {
            let passed = self.pieces.load(Ordering::Relaxed) - seen;
            self.most_passed.fetch_max(passed, Ordering::Relaxed);
            self.first_locks.fetch_add(1, Ordering::Relaxed);
        }
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        state
    }

    fn run(&self) {
        loop {
            let mut state = lock(&self.state);
            while !state.stop && !state.ready() {
                state = self
                    .wake
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            if state.stop {
                return;
            }
            state.run_piece();
            self.publish(&state);
            drop(state);
            #[cfg(test)]
            self.pieces.fetch_add(1, Ordering::Relaxed);
            while self.waiting.load(Ordering::Acquire) != 0 {
                std::thread::yield_now();
            }
        }
    }
}

/// The controller's pending punches and the thread that runs them.
#[derive(Default)]
pub(crate) struct DeferredTrim {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for DeferredTrim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeferredTrim")
            .field("pending", &self.pending())
            .field("started", &self.thread.is_some())
            .finish()
    }
}

impl DeferredTrim {
    /// Whether any punch is pending or running: the one check every WRITE
    /// makes. A `false` read after the last piece means its punch returned.
    pub(crate) fn pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    /// Queue `ranges` of namespace `slot`'s `file` for the thread and return
    /// the ranges dropped at the bound, or `None` (punch inline) when the
    /// thread cannot start.
    fn defer(&mut self, slot: usize, file: &Arc<File>, ranges: &[(u64, u64)]) -> Option<u64> {
        if self.thread.is_none() {
            match self.start() {
                Ok(thread) => self.thread = Some(thread),
                Err(error) => {
                    eprintln!("nvme: TRIM thread not started, punching inline: {error}");
                    return None;
                }
            }
        }
        let mut state = self.shared.lock_first();
        let entry = &mut state.disks[slot];
        if entry
            .as_ref()
            .is_some_and(|disk| !Arc::ptr_eq(&disk.file, file))
        {
            *entry = None;
        }
        let disk = entry.get_or_insert_with(|| Namespace {
            file: Arc::clone(file),
            ranges: PendingRanges::default(),
        });
        let mut dropped = 0;
        for &(start, end) in ranges {
            dropped += u64::from(!disk.ranges.add(start, end));
        }
        self.shared.publish(&state);
        drop(state);
        self.shared.wake.notify_one();
        Some(dropped)
    }

    fn start(&self) -> io::Result<JoinHandle<()>> {
        let shared = Arc::clone(&self.shared);
        std::thread::Builder::new()
            .name("nvme-trim".into())
            .spawn(move || shared.run())
    }

    /// Take `start..end` of namespace `slot` out of the pending punches,
    /// after the piece being punched.
    fn keep(&self, slot: usize, start: u64, end: u64) {
        let mut state = self.shared.lock_first();
        if let Some(disk) = state.disks[slot].as_mut() {
            DSM_STATS.note_dropped(disk.ranges.remove(start, end));
        }
        self.shared.publish(&state);
    }

    /// Drop the pending punches once the piece being punched ends.
    pub(crate) fn discard(&self) {
        let mut state = self.shared.lock_first();
        for disk in state.disks.iter_mut().flatten() {
            disk.ranges = PendingRanges::default();
        }
        self.shared.publish(&state);
    }

    /// Punch every pending range on this thread before returning.
    pub(crate) fn drain(&self) {
        let mut state = self.shared.lock_first();
        while state.run_piece() {}
        self.shared.publish(&state);
    }
}

impl Drop for DeferredTrim {
    fn drop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.shared.lock_first().stop = true;
        self.shared.wake.notify_all();
        let _ = thread.join();
    }
}

#[cfg(test)]
impl DeferredTrim {
    /// Hold or release the thread's pieces; a drain still runs them.
    pub(crate) fn pause(&self, paused: bool) {
        lock(&self.shared.state).paused = paused;
        self.shared.wake.notify_all();
    }

    pub(crate) fn started(&self) -> bool {
        self.thread.is_some()
    }

    /// The pieces the thread has punched.
    pub(crate) fn thread_pieces(&self) -> u64 {
        self.shared.pieces.load(Ordering::Relaxed)
    }

    /// The I/O path's locks so far, and the most pieces one waited for.
    pub(crate) fn first_locks(&self) -> (u64, u64) {
        let shared = &self.shared;
        (
            shared.first_locks.load(Ordering::Relaxed),
            shared.most_passed.load(Ordering::Relaxed),
        )
    }

    /// Namespace `slot`'s pending ranges.
    pub(crate) fn ranges(&self, slot: usize) -> Vec<(u64, u64)> {
        lock(&self.shared.state).disks[slot]
            .as_ref()
            .map_or_else(Vec::new, |disk| disk.ranges.ranges())
    }
}

/// The pending-punch slot of namespace `nsid`.
fn slot(nsid: u32) -> Option<usize> {
    match nsid {
        NSID => Some(0),
        NSID2 => Some(1),
        _ => None,
    }
}

impl NvmeController {
    /// With unlocked I/O on, queue a write-back raw file's deallocated
    /// `ranges` for the punch thread; returns the ranges dropped at the
    /// bound, or `None` to punch them inline.
    pub(super) fn defer_punches(&mut self, nsid: u32, ranges: &[(u64, u64)]) -> Option<u64> {
        if !self.unlocked_io {
            return None;
        }
        let file = match self.backend_for_nsid(nsid) {
            Some(DiskBackend::RawFile(disk)) if disk.write_back => Arc::clone(&disk.file),
            _ => return None,
        };
        self.trim.defer(slot(nsid)?, &file, ranges)
    }

    /// A WRITE just fetched: no pending punch may touch its bytes. Called
    /// only while punches are pending; an invalid range writes nothing.
    pub(crate) fn keep_written(&self, cmd: &SubmissionEntry) {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return;
        };
        if let (Some(slot), Some((start, len))) = (slot(cmd.nsid), transfer_range(cmd, byte_len)) {
            self.trim.keep(slot, start, start + len as u64);
        }
    }
}
