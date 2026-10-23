//! Run-lifetime playback diagnostics, printed as the controller is dropped.
//!
//! Guest PCM advances only when a vCPU exit polls the controller, and one poll
//! plays at most MAX_POLL_ELAPSED of host time. Nothing counted how often
//! running polls came, what buffer the guest programmed, or whether the DMA
//! re-read bytes the guest had not rewritten. These counters record that and
//! nothing else: no method returns a value the DMA, interrupt or register
//! paths act on, and `replace_keeping_diagnostics` keeps them across guest and
//! platform controller resets, so the records cover the whole probe process
//! as the CoreAudio sink's records do.
//!
//! - `hda poll:` running polls by host-time gap since the previous running
//!   poll (the first after RUN only takes the time base), the longest gap, the
//!   polls the cap shortened and the host time it dropped, BDL entries
//!   completed with IOC, the most that one poll completed, and MSIs sent while
//!   the stream ran;
//! - `hda stream:` RUN starts and, for the latest one, its format, cyclic
//!   buffer length, the BDL entries guest memory held at its first poll (bytes
//!   and milliseconds, IOC-flagged entries) and whether DPLBASE enabled the DMA
//!   position buffer; guest reads of SD0 LPIB while the stream ran, and the
//!   host time its running polls spanned;
//! - `hda repeat:` BDL-entry passes whose bytes hash as on that entry's
//!   previous lap (repeat_detector.rs).
//!
//! A controller whose host installed its records prints all of them as it is
//! dropped, after the final report (diagnostics_teardown.rs).

use super::repeat_detector::RepeatDetector;
use super::*;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
#[path = "diagnostics_teardown.rs"]
mod teardown;

/// Records a host PCM sink adds after the controller's own, such as
/// CoreAudio callback timing; read when the controller is dropped.
pub type HdaHostRecords = Box<dyn Fn() -> Vec<String> + Send>;

/// HDA 1.0a allows at most 256 BDL entries.
pub(crate) const MAX_BDL_ENTRIES: usize = 256;
/// Upper bounds of the gap buckets; a longer gap is one the cap shortens.
const GAP_BOUNDS: [Duration; 4] = [
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(50),
    MAX_POLL_ELAPSED,
];

#[derive(Default)]
pub(crate) struct HdaDiagnostics {
    polls: PollCounts,
    stream: StreamShape,
    /// Written from `mmio_read(&self)`.
    lpib_reads: AtomicU64,
    pub(crate) repeats: RepeatDetector,
    host_records: Option<HdaHostRecords>,
}

#[derive(Default)]
struct PollCounts {
    gaps: [u64; 5],
    max_gap: Duration,
    capped: u64,
    capped_lost: Duration,
    running: Duration,
    iocs: u64,
    iocs_this_poll: u64,
    max_iocs_per_poll: u64,
    msis: u64,
}

#[derive(Default)]
struct StreamShape {
    runs: u64,
    fmt: u16,
    cbl: u32,
    entries: u32,
    entry_bytes: Option<(u32, u32)>,
    ioc_entries: u32,
    position_buffer: bool,
}

impl PollCounts {
    /// A running poll begins: fold in the IOCs the previous one completed.
    fn end_poll(&mut self) {
        self.max_iocs_per_poll = self.max_iocs_per_poll.max(self.iocs_this_poll);
        self.iocs_this_poll = 0;
    }

    fn gap(&mut self, gap: Duration) {
        self.end_poll();
        let bucket = GAP_BOUNDS.iter().position(|&bound| gap <= bound);
        self.gaps[bucket.unwrap_or(GAP_BOUNDS.len())] += 1;
        self.max_gap = self.max_gap.max(gap);
        self.running = self.running.saturating_add(gap);
        if let Some(lost) = gap
            .checked_sub(MAX_POLL_ELAPSED)
            .filter(|lost| !lost.is_zero())
        {
            self.capped += 1;
            self.capped_lost = self.capped_lost.saturating_add(lost);
        }
    }
}

impl HdaDiagnostics {
    pub(crate) fn note_ioc(&mut self) {
        self.polls.iocs += 1;
        self.polls.iocs_this_poll += 1;
    }

    pub(crate) fn note_msi(&mut self, running: bool) {
        self.polls.msis += u64::from(running);
    }

    /// Count a guest read that covers SD0 LPIB while the stream runs.
    pub(crate) fn note_register_read(&self, offset: u64, size: u8, running: bool) {
        let end = offset.saturating_add(u64::from(size));
        if running && offset < REG_SD_LPIB + 4 && end > REG_SD_LPIB {
            self.lpib_reads.fetch_add(1, Relaxed);
        }
    }

    fn poll_record(&self) -> String {
        let p = &self.polls;
        let [le10, le20, le50, le100, over] = p.gaps;
        format!(
            "hda poll: polls={} gap_le_10ms={le10} gap_le_20ms={le20} gap_le_50ms={le50} gap_le_100ms={le100} gap_gt_100ms={over} max_gap_ms={} capped_polls={} capped_lost_ms={} ioc_completions={} max_iocs_per_poll={} msis_sent={}",
            p.gaps.iter().sum::<u64>(),
            p.max_gap.as_millis(),
            p.capped,
            p.capped_lost.as_millis(),
            p.iocs,
            p.max_iocs_per_poll.max(p.iocs_this_poll),
            p.msis
        )
    }

    fn stream_record(&self) -> String {
        let s = &self.stream;
        let (rate, channels, bits) = if s.runs == 0 {
            (0, 0, 0)
        } else {
            stream_layout(s.fmt)
        };
        let per_second = stream_bytes_per_second(s.fmt).filter(|_| s.runs > 0);
        let ms = |bytes: u32| millis(bytes, per_second.unwrap_or(0));
        let (min, max) = s.entry_bytes.unwrap_or((0, 0));
        format!(
            "hda stream: runs={} rate_hz={rate} channels={channels} bits={bits} cbl_bytes={} cbl_ms={} entries={} entry_bytes_min={min} entry_bytes_max={max} entry_ms_min={} entry_ms_max={} ioc_entries={} position_buffer={} lpib_reads={} running_ms={}",
            s.runs,
            s.cbl,
            ms(s.cbl),
            s.entries,
            ms(min),
            ms(max),
            s.ioc_entries,
            u8::from(s.position_buffer),
            self.lpib_reads.load(Relaxed),
            self.polls.running.as_millis()
        )
    }
}

/// Rate, channels and container bits as the guest programmed SD0FMT, whether
/// or not the controller can play them.
fn stream_layout(fmt: u16) -> (u32, u16, u16) {
    let bits = match (fmt >> 4) & 0x7 {
        0 => 8,
        1 => 16,
        2 => 20,
        3 => 24,
        4 => 32,
        _ => 0,
    };
    (stream_sample_rate(fmt).unwrap_or(0), (fmt & 0x0f) + 1, bits)
}

/// `bytes` of the stream in milliseconds with three decimals, truncated.
fn millis(bytes: u32, bytes_per_second: u64) -> String {
    let micros = (u64::from(bytes) * 1_000_000)
        .checked_div(bytes_per_second)
        .unwrap_or(0);
    format!("{}.{:03}", micros / 1000, micros % 1000)
}

impl HdaController {
    /// A host-clocked running poll, before it plays: the first after RUN starts
    /// a run, when the DMA only takes its time base; later ones note their gap.
    pub(crate) fn note_diagnostic_poll(&mut self, now: Option<Instant>, mem: &dyn GuestMemoryMut) {
        let Some(now) = now else {
            return;
        };
        match self.last_poll {
            Some(last) => self
                .diagnostics
                .polls
                .gap(now.saturating_duration_since(last)),
            None => self.start_diagnostic_run(mem),
        }
    }

    fn start_diagnostic_run(&mut self, mem: &dyn GuestMemoryMut) {
        let stream = &self.stream;
        let mut shape = StreamShape {
            runs: self.diagnostics.stream.runs + 1,
            fmt: stream.fmt,
            cbl: stream.cbl,
            entries: u32::from(stream.lvi) + 1,
            position_buffer: self.dp_base & 1 != 0,
            ..StreamShape::default()
        };
        for index in 0..shape.entries.min(MAX_BDL_ENTRIES as u32) {
            let mut raw = [0u8; 16];
            let address = stream.bdl.checked_add(u64::from(index) * 16);
            if !address.is_some_and(|address| mem.read_into(address, &mut raw)) {
                continue;
            }
            let length = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let flags = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            shape.ioc_entries += u32::from(flags & BDL_IOC != 0);
            let (min, max) = shape.entry_bytes.unwrap_or((length, length));
            shape.entry_bytes = Some((min.min(length), max.max(length)));
        }
        self.diagnostics.repeats.start_run(shape.entries as usize);
        self.diagnostics.polls.end_poll();
        self.diagnostics.stream = shape;
    }

    /// The `hda poll:`, `hda stream:` and `hda repeat:` records, then the
    /// records the host sink adds.
    pub fn diagnostic_records(&self) -> Vec<String> {
        let diagnostics = &self.diagnostics;
        let mut records = vec![
            diagnostics.poll_record(),
            diagnostics.stream_record(),
            diagnostics.repeats.record(),
        ];
        if let Some(host_records) = &diagnostics.host_records {
            records.extend(host_records());
        }
        records
    }

    /// Install or clear the host sink's records; installed, all print at drop.
    pub fn set_diagnostic_host_records(&mut self, records: Option<HdaHostRecords>) {
        self.diagnostics.host_records = records;
    }

    /// Reinitialize the controller as `fresh`, keeping run-lifetime diagnostics.
    pub(crate) fn replace_keeping_diagnostics(&mut self, fresh: Self) {
        let diagnostics = std::mem::take(&mut self.diagnostics);
        *self = fresh;
        self.diagnostics = diagnostics;
    }
}
