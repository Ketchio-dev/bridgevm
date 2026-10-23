//! Stale-replay detection for the final report's `hda repeat:` record.
//!
//! A poll that plays more than the guest has rewritten since the previous lap
//! re-reads old buffer bytes and hands them to the sink as good PCM; no other
//! counter can see that. This hashes every whole BDL-entry pass, from the
//! entry's first byte through its last read contiguously while the stream
//! runs, and compares it with that entry's previous lap in the same run:
//!
//! - `passes`: whole passes of the first 256 entries;
//! - `silent_passes`: passes whose bytes were all zero, never compared;
//! - `compared_passes`: other passes whose entry has a previous lap this run;
//! - `repeat_passes`: compared passes that hashed as their previous lap;
//! - `max_repeat_run`: the most repeat passes in a row.
//!
//! A pass joined mid-entry (a resume at the saved cursor, or a wrap at CBL) is
//! skipped, and each RUN start forgets earlier laps. The hash is 64-bit FNV-1a,
//! a diagnostic and not a security value. Content that is identical on every
//! lap, such as a tone whose period divides the cyclic buffer, counts as a
//! repeat too; a test signal must avoid that.

use super::diagnostics::MAX_BDL_ENTRIES;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Default)]
pub(crate) struct RepeatDetector {
    laps: Vec<Option<u64>>,
    pass: Option<Pass>,
    passes: u64,
    silent: u64,
    compared: u64,
    repeats: u64,
    run: u64,
    max_run: u64,
}

struct Pass {
    index: u16,
    next_offset: u32,
    hash: u64,
    audible: bool,
}

impl RepeatDetector {
    /// A RUN start: earlier laps and any partial pass are forgotten.
    pub(crate) fn start_run(&mut self, entries: usize) {
        self.laps.clear();
        self.laps.resize(entries.min(MAX_BDL_ENTRIES), None);
        self.pass = None;
        self.run = 0;
    }

    /// One DMA chunk of BDL entry `index`, `length` bytes long, at `offset`.
    pub(crate) fn note_chunk(&mut self, index: u16, offset: u32, length: u32, chunk: &[u8]) {
        let mut pass = match self.pass.take() {
            Some(pass) if pass.index == index && pass.next_offset == offset => pass,
            _ if offset == 0 => Pass {
                index,
                next_offset: 0,
                hash: FNV_OFFSET,
                audible: false,
            },
            _ => return,
        };
        for &byte in chunk {
            pass.hash = (pass.hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
            pass.audible |= byte != 0;
        }
        let read = u32::try_from(chunk.len()).unwrap_or(u32::MAX);
        pass.next_offset = offset.saturating_add(read);
        if pass.next_offset >= length {
            self.finish(pass);
        } else {
            self.pass = Some(pass);
        }
    }

    fn finish(&mut self, pass: Pass) {
        let Some(lap) = self.laps.get_mut(usize::from(pass.index)) else {
            self.run = 0;
            return;
        };
        let previous = lap.replace(pass.hash);
        self.passes += 1;
        let repeat = match previous {
            _ if !pass.audible => {
                self.silent += 1;
                false
            }
            Some(previous) => {
                self.compared += 1;
                previous == pass.hash
            }
            None => false,
        };
        if repeat {
            self.repeats += 1;
            self.run += 1;
            self.max_run = self.max_run.max(self.run);
        } else {
            self.run = 0;
        }
    }

    pub(crate) fn record(&self) -> String {
        format!(
            "hda repeat: passes={} silent_passes={} compared_passes={} repeat_passes={} max_repeat_run={}",
            self.passes, self.silent, self.compared, self.repeats, self.max_run
        )
    }
}
