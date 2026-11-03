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
//! ([`NvmeController::take_io`]), runs the transfers unlocked ([`NvmeIo::run`])
//! and publishes the results to the flush worker's `finished` list; it then
//! locks again and its drain posts the completions (`complete_flushes`), each
//! only after its transfer returned. A FUA or write-cache-off WRITE goes on to
//! the worker's sync from there and completes after it, as inline.
//!
//! Every wait for the flushes in flight covers these jobs too: a reset,
//! shutdown, admin command (queue deletion among them), restore or export
//! waits under the lock, and the vCPU running the transfer publishes without
//! it, so nothing deadlocks, and a reset returns only after any READ has
//! written guest RAM. The vCPU carries no job across the unlock: completions
//! exist only in `finished`, which those paths post or drop under the lock,
//! so none can reach a queue deleted or re-created meanwhile.

use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Arc;

use super::flush_worker::{Finished, FlushJob, Shared};
use super::*;
use crate::fwcfg::GuestMemoryMut;

/// One READ or WRITE, checked and resolved under the platform lock.
pub(crate) struct IoJob {
    done: FlushJob,
    file: Arc<File>,
    write: bool,
    sync_next: bool,
    offset: u64,
    /// Host address and length of each contiguous guest segment.
    iov: Vec<(usize, usize)>,
}

impl IoJob {
    fn transfer(&self) -> u16 {
        let mut offset = self.offset;
        for &(addr, len) in &self.iov {
            // SAFETY: `host_ptr` resolved [addr, addr + len) inside guest RAM
            // under the platform lock. Guest RAM stays mapped while vCPUs run,
            // and a reset waits for this job before guest RAM is reused. The
            // guest does not touch a buffer while its command is outstanding.
            let result = if self.write {
                let src = unsafe { std::slice::from_raw_parts(addr as *const u8, len) };
                self.file.write_all_at(src, offset)
            } else {
                let dst = unsafe { std::slice::from_raw_parts_mut(addr as *mut u8, len) };
                self.file.read_exact_at(dst, offset)
            };
            if result.is_err() {
                return SC_INVALID_FIELD;
            }
            offset += len as u64;
        }
        SC_SUCCESS
    }
}

/// The READs and WRITEs a doorbell queued. Run them after dropping the
/// platform lock; the next drain under the lock posts their completions.
#[must_use = "a waiting reset or admin command blocks until the jobs run"]
pub struct NvmeIo {
    jobs: Vec<IoJob>,
    shared: Arc<Shared>,
    #[cfg(test)]
    gate: Arc<test_gate::IoGate>,
}

impl NvmeIo {
    /// Run the transfers in order and publish their results.
    pub fn run(mut self) {
        self.run_jobs();
    }

    fn run_jobs(&mut self) {
        let jobs = std::mem::take(&mut self.jobs);
        let done: Vec<Finished> = jobs
            .iter()
            .map(|job| {
                #[cfg(test)]
                self.gate.pass();
                let status = job.transfer();
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
    /// Jobs taken but not run (an exit that stopped its vCPU first) run here:
    /// every wait for the jobs in flight counts them.
    fn drop(&mut self) {
        if !self.jobs.is_empty() {
            self.run_jobs();
        }
    }
}

impl NvmeController {
    /// Live VMMs enable this; tests and checkpointing keep the inline path.
    pub fn set_unlocked_io(&mut self, enabled: bool) {
        self.unlocked_io = enabled;
    }

    /// Queue an I/O-queue READ or WRITE to run without the platform lock.
    /// `false` leaves it to the inline path, which reports any error: unlocked
    /// I/O off, not a write-back raw file, no host pointers, or an invalid
    /// range or PRP list.
    pub(crate) fn defer_io(
        &mut self,
        sqid: u16,
        cqid: u16,
        cmd: &SubmissionEntry,
        mem: &dyn GuestMemoryMut,
    ) -> bool {
        let write = match cmd.opcode {
            NVM_OP_READ => false,
            NVM_OP_WRITE => true,
            _ => return false,
        };
        if !self.unlocked_io || sqid == 0 || !self.direct_dma_enabled {
            return false;
        }
        let (file, byte_len) = match self.backend_for_nsid(cmd.nsid) {
            Some(DiskBackend::RawFile(disk)) if disk.write_back => (Arc::clone(&disk.file), disk.len),
            _ => return false,
        };
        let Some((offset, len)) = transfer_range(cmd, byte_len) else {
            return false;
        };
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        let mut segments = std::mem::take(&mut self.io_segments_scratch);
        spans.clear();
        segments.clear();
        let iov = if prp_spans_into(cmd, len, mem, &mut spans, &mut self.io_scratch) {
            coalesce_spans_into(&spans, &mut segments);
            segments
                .iter()
                .map(|&(gpa, len)| mem.host_ptr(gpa, len).map(|ptr| (ptr as usize, len)))
                .collect::<Option<Vec<_>>>()
        } else {
            None
        };
        spans.clear();
        segments.clear();
        self.prp_spans_scratch = spans;
        self.io_segments_scratch = segments;
        let Some(iov) = iov else {
            return false;
        };
        let sync_next = write && self.write_needs_sync(cmd);
        let worker = self.flush_worker.get_or_insert_with(FlushWorker::default);
        worker.in_flight += 1;
        worker.io.push(IoJob {
            done: FlushJob::io(sqid, cqid, *cmd),
            file,
            write,
            sync_next,
            offset,
            iov,
        });
        self.hold_completion_slot(cqid);
        true
    }

    /// The READs and WRITEs queued since the last call, for the doorbell vCPU
    /// to run once it has dropped the platform lock.
    pub fn take_io(&mut self) -> Option<NvmeIo> {
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

    /// Jobs nobody took (only a caller that skips `take_io` leaves any) run
    /// here, under the lock, before a wait would block on them forever.
    pub(super) fn run_untaken_io(&mut self) {
        if let Some(io) = self.take_io() {
            io.run();
        }
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
    }

    impl IoGate {
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
