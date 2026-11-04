//! Order between the WRITEs and deallocations that run without the platform
//! lock (unlocked_io.rs). NVMe orders no commands in flight, but inline they
//! ran one at a time in fetch order, and the vCPUs' workers run theirs at
//! once: a punch that landed after a later WRITE to the same blocks would
//! drop that WRITE's data. QEMU's block layer serialises overlapping discards
//! and writes as well.
//!
//! Each WRITE and deallocation takes a ticket in fetch order when the
//! doorbell checks it, under the lock. Before it runs it waits, without the
//! lock, until no earlier job of the other kind with overlapping bytes on its
//! namespace is left. So a punch never overtakes an earlier WRITE, and a
//! later WRITE never lands before an earlier punch. READs, and WRITEs among
//! themselves, stay unordered, as NVMe allows.
//!
//! Nothing deadlocks: a job waits only for earlier tickets, and each vCPU's
//! worker runs its batches in fetch order, so the earliest job left never
//! waits. A job leaves once it has run, or has failed without running.

use std::sync::{Arc, Condvar, Mutex, PoisonError};

use super::flush_worker::lock;

/// The bytes a job changes.
pub(super) enum Change {
    /// A WRITE's byte range.
    Write(u64, u64),
    /// A deallocation's byte ranges.
    Punch(Arc<[(u64, u64)]>),
}

impl Change {
    /// A WRITE and a punch with bytes in common.
    fn conflicts(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Write(start, end), Self::Punch(ranges))
            | (Self::Punch(ranges), Self::Write(start, end)) => {
                ranges.iter().any(|&(from, to)| from < *end && *start < to)
            }
            _ => false,
        }
    }
}

struct Job {
    ticket: u64,
    nsid: u32,
    change: Change,
}

#[derive(Default)]
struct State {
    next: u64,
    /// The jobs left, in ticket order.
    jobs: Vec<Job>,
    waiting: usize,
}

impl State {
    fn blocked(&self, ticket: u64) -> bool {
        let Ok(at) = self.jobs.binary_search_by_key(&ticket, |job| job.ticket) else {
            return false;
        };
        let job = &self.jobs[at];
        self.jobs[..at]
            .iter()
            .any(|earlier| earlier.nsid == job.nsid && earlier.change.conflicts(&job.change))
    }
}

/// The WRITEs and deallocations queued and not yet run.
#[derive(Default)]
pub(super) struct IoOrder {
    state: Mutex<State>,
    changed: Condvar,
}

impl IoOrder {
    /// Under the platform lock, in fetch order: the job's ticket.
    pub(super) fn enter(&self, nsid: u32, change: Change) -> u64 {
        let mut state = lock(&self.state);
        let ticket = state.next;
        state.next += 1;
        state.jobs.push(Job {
            ticket,
            nsid,
            change,
        });
        ticket
    }

    /// Without the platform lock: return once no earlier job conflicts.
    pub(super) fn wait_turn(&self, ticket: u64) {
        let mut state = lock(&self.state);
        while state.blocked(ticket) {
            state.waiting += 1;
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            state.waiting -= 1;
        }
    }

    /// The job ran, or failed unrun: later ones may go.
    pub(super) fn leave(&self, ticket: u64) {
        let mut state = lock(&self.state);
        if let Ok(at) = state.jobs.binary_search_by_key(&ticket, |job| job.ticket) {
            state.jobs.remove(at);
        }
        let wake = state.waiting > 0;
        drop(state);
        if wake {
            self.changed.notify_all();
        }
    }

    /// Jobs waiting for their turn.
    #[cfg(test)]
    pub(super) fn waiting(&self) -> usize {
        lock(&self.state).waiting
    }
}
