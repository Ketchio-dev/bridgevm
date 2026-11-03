//! Hold the playback DMA for a guest that services an IOC late.
//!
//! Windows refills its two-entry, 20 ms cyclic buffer one 10 ms entry per IOC
//! and picks the entry from the position it reads at the SD0 LPIB alias. Under
//! host load its vCPUs sometimes serviced an IOC 9-20 ms late (live: no HDA poll
//! and no vCPU exit for 9-14 ms). By then the DMA had reached, or come within
//! about the 32-byte FIFO of, the entry that IOC freed: the guest took that
//! entry as already playing and wrote the next one, so the freed entry played
//! its previous lap again, a 10 ms stale repeat (`hda repeat:`).
//!
//! So while a guest that reads the position has not read it since the entry
//! after the cursor last completed, the DMA stops 1 ms short of the end of the
//! current entry, where that read still finds the freed entry free. The host
//! time held becomes a debt of at most 10 ms that later polls repay at up to
//! 1.5x speed, so the host sink's ring dips by at most that and the long-run
//! rate is unchanged. A poll whose rest does not fit in that debt, as after a
//! long host stall, plays on as before: holding part of it would only take
//! audio from the ring. A guest that has not read the position this run, or
//! that enabled the DMA position buffer, is never held.

use super::diagnostics::MAX_BDL_ENTRIES;
use super::*;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// How far short of an entry's end a late guest's DMA stops.
const GUARD: Duration = Duration::from_millis(1);
/// The most host time the DMA holds back before it plays on regardless.
const MAX_DEBT: Duration = Duration::from_millis(10);

#[derive(Debug, Default)]
pub(crate) struct LateGuestHold {
    /// Guest reads of SD0 LPIB or its alias while RUN is set; written from
    /// `mmio_read(&self)`, always under the platform lock.
    reads: AtomicU64,
    /// `reads` when the current run started.
    run_base: u64,
    /// `reads` when each entry last completed this run; `u64::MAX` before.
    completed: Vec<u64>,
    /// Stream bytes held back and not yet repaid.
    pub(crate) debt: usize,
}

/// Whether a guest read of `size` bytes at `offset` covers SD0 LPIB or its alias.
pub(crate) fn reads_position(offset: u64, size: u8) -> bool {
    let end = offset.saturating_add(u64::from(size));
    [REG_SD_LPIB, REG_SD_LPIB_ALIAS]
        .iter()
        .any(|&register| offset < register + 4 && end > register)
}

impl LateGuestHold {
    pub(crate) fn start_run(&mut self, entries: usize) {
        self.run_base = self.reads.load(Relaxed);
        self.completed = vec![u64::MAX; entries.min(MAX_BDL_ENTRIES)];
        self.debt = 0;
    }

    pub(crate) fn note_read(&self) {
        self.reads.fetch_add(1, Relaxed);
    }

    pub(crate) fn note_completed(&mut self, index: u16) {
        let reads = self.reads.load(Relaxed);
        if let Some(completed) = self.completed.get_mut(usize::from(index)) {
            *completed = reads;
        }
    }

    /// The guest reads the position but has not since entry `index` completed.
    fn late_for(&self, index: u16) -> bool {
        let reads = self.reads.load(Relaxed);
        reads != self.run_base && self.completed.get(usize::from(index)) == Some(&reads)
    }
}

impl HdaController {
    /// Of the `remaining` bytes of the current entry, those the DMA may read
    /// now: all, or 1 ms fewer while the guest is late for the next entry.
    pub(crate) fn late_guest_room(&self, remaining: usize) -> usize {
        let next = if self.stream.bdl_index >= self.stream.lvi {
            0
        } else {
            self.stream.bdl_index + 1
        };
        if self.dp_base & 1 != 0 || !self.hold.late_for(next) {
            return remaining;
        }
        remaining.saturating_sub(self.stream_bytes(GUARD))
    }

    /// Hold all of `budget` as debt if it fits under MAX_DEBT, returning 0;
    /// else hold nothing and return it to play on.
    pub(crate) fn hold_for_late_guest(&mut self, budget: usize) -> usize {
        if self.hold.debt + budget > self.stream_bytes(MAX_DEBT) {
            return budget;
        }
        self.hold.debt += budget;
        0
    }

    /// Extra bytes a poll with `budget` plays to repay the debt: up to half.
    pub(crate) fn repay_hold(&mut self, budget: usize, frame_bytes: usize) -> usize {
        let repay = self.hold.debt.min(budget / 2) / frame_bytes * frame_bytes;
        self.hold.debt -= repay;
        repay
    }

    /// `duration` of the current stream format in bytes, whole frames.
    fn stream_bytes(&self, duration: Duration) -> usize {
        let per_second = stream_bytes_per_second(self.stream.fmt).unwrap_or(0);
        let frame = u64::from(stream_frame_bytes(self.stream.fmt).unwrap_or(1).max(1));
        let bytes = per_second * duration.as_millis() as u64 / 1000;
        (bytes / frame * frame) as usize
    }
}
