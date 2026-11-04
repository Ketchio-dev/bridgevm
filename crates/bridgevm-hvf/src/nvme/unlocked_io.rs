//! NVM READ and WRITE without the platform lock. A raw-file `pread` or
//! `pwrite` takes tens of ms and more when the host disk is busy, and it ran
//! inside the doorbell write, under the platform lock, so the other vCPUs and
//! the HDA tick stalled behind it (one quiet run: 59 holds over 20 ms, the
//! longest 150 ms, 300 stale audio repeats).
//!
//! With unlocked I/O on, `process_sq` checks an I/O-queue READ or WRITE of a
//! write-back raw file, decodes its PRPs and resolves them to host pointers,
//! holds its completion slot and counts it in flight with the flushes, all
//! under the lock. The doorbell vCPU takes the jobs before it unlocks
//! ([`NvmeController::take_io`]) and hands them to its transfer thread
//! ([`NvmeIoWorker`](super::NvmeIoWorker), io_worker.rs), which runs them in
//! its view of guest RAM ([`NvmeIo::run`]) and publishes the results to the
//! flush worker's `finished` list. A vCPU whose transfers finished within
//! 1 ms locks again and its drain posts the completions (`complete_flushes`);
//! otherwise it returns to the guest and the drain that `FLUSH_DONE_HOOK` asks
//! for posts them, each only after its transfer returned. A FUA or
//! write-cache-off WRITE goes on to the flush worker's sync from there and
//! completes after it, as inline.
//!
//! A Dataset Management Deallocate (Windows' TRIM, dataset_management.rs)
//! goes the same way: inline, the punches of one 1 GiB delete held the lock
//! for up to 24 ms (PR #497). The doorbell reads and checks its range list
//! under the lock, so an invalid one completes inline with its error, and the
//! job only punches. WRITEs and deallocations with bytes in common run in
//! fetch order (io_order.rs).
//!
//! Taking the jobs is `unsafe` ([`NvmeController::take_io`]): they run, on
//! another thread and after the lock is dropped, through the raw host pointers
//! that the running memory view's `host_ptr` returns, so the caller promises
//! that view stays mapped until each transfer returns. A job dropped unrun
//! touches no guest RAM.
//!
//! Every wait for the flushes in flight covers these jobs too: a reset,
//! shutdown, admin command (queue deletion among them), restore or export
//! waits under the lock, and the thread running the transfer publishes
//! without it, so nothing deadlocks, and a reset returns only after any READ
//! has written guest RAM. Completions exist only in `finished`, which those
//! paths post or drop under the lock, so none can reach a queue deleted or
//! re-created meanwhile.

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use super::dataset_management::punch_unlocked;
use super::flush_worker::{Finished, FlushJob, Shared};
use super::io_order::Change;
use super::*;
use crate::fwcfg::GuestMemoryMut;

/// One READ, WRITE or deallocation, checked under the platform lock.
pub(crate) struct IoJob {
    done: FlushJob,
    file: Arc<File>,
    kind: Kind,
    sync_next: bool,
    offset: u64,
    /// Guest-physical address and length of each contiguous segment.
    segments: Vec<(u64, usize)>,
    /// A WRITE's or deallocation's place in fetch order (io_order.rs).
    ticket: Option<u64>,
}

enum Kind {
    Read,
    Write,
    /// Punch these byte ranges.
    Deallocate(Arc<[(u64, u64)]>),
}

impl IoJob {
    /// Guest RAM is shared with the guest and with other vCPUs' transfers,
    /// so it is never borrowed as a Rust slice: `pread`/`pwrite` copy
    /// straight to and from the mapped pages, as DMA would.
    fn transfer(&self, mem: &dyn GuestMemoryMut) -> u16 {
        let fd = self.file.as_raw_fd();
        let write = matches!(self.kind, Kind::Write);
        let mut offset = self.offset;
        for &(gpa, len) in &self.segments {
            let Some(ptr) = mem.host_ptr(gpa, len) else {
                return SC_INVALID_FIELD;
            };
            if copy_all(fd, ptr, len, offset, write).is_err() {
                return SC_INVALID_FIELD;
            }
            offset += len as u64;
        }
        SC_SUCCESS
    }
}

/// `pread` into, or `pwrite` from, the `len` bytes at `ptr`, across short
/// transfers and EINTR.
fn copy_all(fd: RawFd, ptr: *mut u8, len: usize, offset: u64, write: bool) -> io::Result<()> {
    let mut done = 0;
    while done < len {
        let at = libc::off_t::try_from(offset + done as u64)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: `host_ptr` resolved [ptr, ptr + len) in `mem`, which
        // `take_io`'s caller promised stays valid for reads and writes until
        // this transfer returns; the kernel copies to or from it without a
        // Rust reference to it.
        let n = unsafe {
            let at_ptr = ptr.add(done).cast::<libc::c_void>();
            if write {
                libc::pwrite(fd, at_ptr, len - done, at)
            } else {
                libc::pread(fd, at_ptr, len - done, at)
            }
        };
        match n {
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n if n < 0 => {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
            n => done += n as usize,
        }
    }
    Ok(())
}

/// The READs, WRITEs and deallocations a doorbell queued, from the `unsafe`
/// [`NvmeController::take_io`] only. Run them after dropping the platform
/// lock, on the vCPU's [`NvmeIoWorker`](super::NvmeIoWorker); the next drain
/// under the lock posts their completions.
#[must_use = "a waiting reset or admin command blocks until the jobs finish"]
pub struct NvmeIo {
    jobs: Vec<IoJob>,
    shared: Arc<Shared>,
    #[cfg(test)]
    gate: Arc<test_gate::IoGate>,
}

impl NvmeIo {
    /// Run the jobs in order, each in its turn (io_order.rs), and publish
    /// their results. `mem` is the calling thread's view of the guest RAM the
    /// doorbell checked them in.
    pub fn run(mut self, mem: &dyn GuestMemoryMut) {
        let jobs = std::mem::take(&mut self.jobs);
        let order = &self.shared.order;
        self.publish(jobs, |job| {
            if let Some(ticket) = job.ticket {
                order.wait_turn(ticket);
            }
            let status = match &job.kind {
                Kind::Deallocate(ranges) => punch_unlocked(&job.file, ranges),
                Kind::Read | Kind::Write => {
                    #[cfg(test)]
                    self.gate.pass();
                    job.transfer(mem)
                }
            };
            if let Some(ticket) = job.ticket {
                order.leave(ticket);
            }
            status
        });
    }

    fn publish(&self, jobs: Vec<IoJob>, mut status_of: impl FnMut(&IoJob) -> u16) {
        let done: Vec<Finished> = jobs
            .iter()
            .map(|job| {
                let status = status_of(job);
                Finished {
                    job: job.done,
                    status,
                    sync_next: job.sync_next && status == SC_SUCCESS,
                }
            })
            .collect();
        self.shared.publish(done);
    }
}

impl Drop for NvmeIo {
    /// Jobs never run (an exit that stopped its vCPU first) fail without
    /// touching guest RAM: every wait for the jobs in flight counts them.
    fn drop(&mut self) {
        if !self.jobs.is_empty() {
            let jobs = std::mem::take(&mut self.jobs);
            let order = &self.shared.order;
            self.publish(jobs, |job| {
                if let Some(ticket) = job.ticket {
                    order.leave(ticket);
                }
                SC_INTERNAL_DEVICE_ERROR
            });
        }
    }
}

impl NvmeController {
    /// Live VMMs enable this; tests and checkpointing keep the inline path.
    pub fn set_unlocked_io(&mut self, enabled: bool) {
        self.unlocked_io = enabled;
    }

    /// Queue an I/O-queue READ, WRITE or Deallocate to run without the
    /// platform lock. `false` leaves it to the inline path, which reports any
    /// error: unlocked I/O off, not a write-back raw file, no host pointers,
    /// an invalid range, PRP list or range list, or nothing to deallocate.
    /// A WRITE and a Deallocate share the conditions, so on one disk both run
    /// here or both inline.
    pub(crate) fn defer_io(
        &mut self,
        sqid: u16,
        cqid: u16,
        cmd: &SubmissionEntry,
        mem: &dyn GuestMemoryMut,
    ) -> bool {
        if !matches!(
            cmd.opcode,
            NVM_OP_READ | NVM_OP_WRITE | NVM_OP_DATASET_MANAGEMENT
        ) || !self.unlocked_io
            || sqid == 0
            || !self.direct_dma_enabled
        {
            return false;
        }
        let (file, byte_len) = match self.backend_for_nsid(cmd.nsid) {
            Some(DiskBackend::RawFile(disk)) if disk.write_back => {
                (Arc::clone(&disk.file), disk.len)
            }
            _ => return false,
        };
        let (kind, change, offset, segments) = if cmd.opcode == NVM_OP_DATASET_MANAGEMENT {
            let Some(ranges) = self.deallocation(cmd, byte_len, mem) else {
                return false;
            };
            let ranges: Arc<[(u64, u64)]> = ranges.into();
            let change = Change::Punch(Arc::clone(&ranges));
            (Kind::Deallocate(ranges), Some(change), 0, Vec::new())
        } else {
            let Some((offset, len)) = transfer_range(cmd, byte_len) else {
                return false;
            };
            let Some(segments) = self.host_segments(cmd, len, mem) else {
                return false;
            };
            if cmd.opcode == NVM_OP_WRITE {
                let change = Change::Write(offset, offset + len as u64);
                (Kind::Write, Some(change), offset, segments)
            } else {
                (Kind::Read, None, offset, segments)
            }
        };
        let sync_next = matches!(kind, Kind::Write) && self.write_needs_sync(cmd);
        let worker = self.flush_worker.get_or_insert_with(FlushWorker::default);
        let ticket = change.map(|change| worker.shared.order.enter(cmd.nsid, change));
        worker.in_flight += 1;
        worker.io.push(IoJob {
            done: FlushJob::io(sqid, cqid, *cmd),
            file,
            kind,
            sync_next,
            offset,
            segments,
            ticket,
        });
        self.hold_completion_slot(cqid);
        true
    }

    /// The guest segments of a `len`-byte transfer, if its PRPs decode and
    /// every segment has a host pointer.
    fn host_segments(
        &mut self,
        cmd: &SubmissionEntry,
        len: usize,
        mem: &dyn GuestMemoryMut,
    ) -> Option<Vec<(u64, usize)>> {
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        spans.clear();
        segments.clear();
        let resolved = prp_spans_into(cmd, len, mem, &mut spans, &mut self.io_scratch) && {
            coalesce_spans_into(&spans, &mut segments);
            segments
                .iter()
                .all(|&(gpa, len)| mem.host_ptr(gpa, len).is_some())
        };
        let job_segments = resolved.then(|| segments.clone());
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        job_segments
    }

    /// The READs, WRITEs and deallocations queued since the last call, for
    /// the doorbell vCPU to run once it has dropped the platform lock.
    ///
    /// # Safety
    ///
    /// The transfers read and write guest RAM through the raw pointers that
    /// `host_ptr` returns when they run, possibly on another thread and long
    /// after this call. Run them, with [`NvmeIo::run`] or on an
    /// [`NvmeIoWorker`](super::NvmeIoWorker), only in a memory view whose
    /// `host_ptr` pointers stay valid for reads and writes of the requested
    /// length until the transfer returns; a worker's view must stay valid
    /// until the worker is dropped, which joins its thread. Dropping the
    /// result unrun is always sound: its jobs fail without touching guest RAM.
    ///
    /// Safe code cannot take them:
    ///
    /// ```compile_fail
    /// use bridgevm_hvf::nvme::{NvmeController, NvmeIo};
    /// fn take(ctrl: &mut NvmeController) -> Option<NvmeIo> {
    ///     ctrl.take_io()
    /// }
    /// ```
    ///
    /// ```
    /// use bridgevm_hvf::nvme::{NvmeController, NvmeIo};
    /// fn take(ctrl: &mut NvmeController) -> Option<NvmeIo> {
    ///     // SAFETY: (example) the caller runs them only in guest RAM it keeps mapped.
    ///     unsafe { ctrl.take_io() }
    /// }
    /// ```
    pub unsafe fn take_io(&mut self) -> Option<NvmeIo> {
        let worker = self.flush_worker.as_mut()?;
        if worker.io.is_empty() {
            return None;
        }
        Some(NvmeIo {
            jobs: std::mem::take(&mut worker.io),
            shared: Arc::clone(&worker.shared),
            #[cfg(test)]
            gate: Arc::clone(&self.io_gate),
        })
    }

    /// Jobs nobody took (only a caller that skips `take_io` leaves any) fail
    /// here, before a wait would block on them forever.
    pub(super) fn fail_untaken_io(&mut self) {
        // SAFETY: dropped unrun, so the jobs fail without touching guest RAM.
        drop(unsafe { self.take_io() });
    }
}

#[cfg(test)]
pub(crate) mod test_gate {
    use crate::nvme::flush_worker::lock;
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    /// Holds transfers while a test checks the state around them; open
    /// unless a test closes it. Inline transfers pass it too.
    #[derive(Debug, Default)]
    pub(crate) struct IoGate {
        /// (closed, transfers that reached the gate)
        state: Mutex<(bool, usize)>,
        changed: Condvar,
        /// The name of the thread each transfer ran on.
        threads: Mutex<Vec<String>>,
    }

    impl IoGate {
        pub(crate) fn threads(&self) -> Vec<String> {
            lock(&self.threads).clone()
        }

        pub(crate) fn close(&self) {
            lock(&self.state).0 = true;
        }

        pub(crate) fn open(&self) {
            lock(&self.state).0 = false;
            self.changed.notify_all();
        }

        /// Wait up to 5 s until `n` transfers have reached the gate.
        pub(crate) fn reached(&self, n: usize) -> bool {
            let give_up = Instant::now() + Duration::from_secs(5);
            let mut state = lock(&self.state);
            while state.1 < n {
                let left = give_up.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return false;
                }
                state = self
                    .changed
                    .wait_timeout(state, left)
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0;
            }
            true
        }

        pub(crate) fn pass(&self) {
            let thread = std::thread::current().name().unwrap_or_default().to_owned();
            lock(&self.threads).push(thread);
            let mut state = lock(&self.state);
            state.1 += 1;
            self.changed.notify_all();
            while state.0 {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
    }
}
