//! Run-lifetime playback diagnostics (diagnostics.rs): the cadence of running
//! polls, the programmed stream and LPIB reads, kept across controller resets.

use super::super::*;
use super::helpers::{write, RAM_BASE};
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const BDL: u64 = RAM_BASE + 0x1000;
pub(super) const PCM: u64 = RAM_BASE + 0x4000;
pub(super) const PERIOD: u32 = 1_920; // 10 ms of 48 kHz stereo s16le.
pub(super) const TWO_PERIODS: [(u32, bool); 2] = [(PERIOD, true), (PERIOD, true)];
const DPL: u64 = RAM_BASE + 0x3000;
const MSI: (u64, u32) = (0x0808_0040, 0x41);
const RUN_WITH_IOC: u64 = (SDCTL_IOCE | SDCTL_RUN) as u64;

/// Program `entries` (bytes, IOC) as the BDL of a 48 kHz stereo s16le stream
/// with stream interrupts enabled, then set RUN. Nothing has polled yet.
pub(super) fn running_stream(
    entries: &[(u32, bool)],
    position_buffer: bool,
) -> (HdaController, FlatGuestRam) {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    let mut mem = FlatGuestRam::new(RAM_BASE, 0x20000);
    let mut address = PCM;
    for (index, &(length, ioc)) in entries.iter().enumerate() {
        let mut descriptor = [0u8; 16];
        descriptor[..8].copy_from_slice(&address.to_le_bytes());
        descriptor[8..12].copy_from_slice(&length.to_le_bytes());
        descriptor[12..16].copy_from_slice(&u32::from(ioc).to_le_bytes());
        assert!(mem.write_bytes(BDL + 16 * index as u64, &descriptor));
        address += u64::from(length);
    }
    let cbl: u32 = entries.iter().map(|&(length, _)| length).sum();
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 1);
    if position_buffer {
        write(&mut ctrl, &mut mem, REG_DPLBASE, 4, DPL | 1);
    }
    write(&mut ctrl, &mut mem, REG_SD_BDPL, 4, BDL);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, u64::from(cbl));
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, entries.len() as u64 - 1);
    write(&mut ctrl, &mut mem, REG_SD_FMT, 2, 0x0011);
    let interrupts = u64::from(INTCTL_GIE | INTCTL_STREAM0);
    write(&mut ctrl, &mut mem, REG_INTCTL, 4, interrupts);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 3, RUN_WITH_IOC);
    (ctrl, mem)
}

/// One host-clocked poll at each offset from `t0`, as vCPU exits drive them.
pub(super) fn poll_at(ctrl: &mut HdaController, mem: &mut FlatGuestRam, t0: Instant, ms: &[u64]) {
    for &offset in ms {
        ctrl.poll(mem, Some(t0 + Duration::from_millis(offset)));
    }
}

#[test]
fn poll_record_buckets_running_gaps_and_counts_the_time_the_cap_drops() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    // A time-base poll, then gaps of 5, 5 and 250 ms: 5 + 5 + 100 ms play.
    poll_at(&mut ctrl, &mut mem, Instant::now(), &[0, 5, 10, 260]);
    assert_eq!(
        ctrl.diagnostic_records()[0],
        "hda poll: polls=3 gap_le_10ms=2 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 \
         gap_gt_100ms=1 max_gap_ms=250 capped_polls=1 capped_lost_ms=150 \
         ioc_completions=11 max_iocs_per_poll=10 msis_sent=0"
    );
}

#[test]
fn poll_gap_buckets_include_their_upper_bound_and_the_cap_starts_above_100_ms() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    ctrl.poll(&mut mem, Some(t0));
    let mut at = Duration::ZERO;
    let gaps = [10, 11, 20, 21, 50, 51, 100].map(Duration::from_millis);
    for gap in gaps.into_iter().chain([Duration::from_micros(100_001)]) {
        at += gap;
        ctrl.poll(&mut mem, Some(t0 + at));
    }
    let poll = &ctrl.diagnostic_records()[0];
    assert!(
        poll.starts_with(
            "hda poll: polls=8 gap_le_10ms=1 gap_le_20ms=2 gap_le_50ms=2 gap_le_100ms=2 \
             gap_gt_100ms=1 max_gap_ms=100 capped_polls=1 capped_lost_ms=0 "
        ),
        "{poll}"
    );
}

#[test]
fn poll_record_counts_ioc_completions_against_the_msis_sent() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    ctrl.poll(&mut mem, Some(t0));
    let mut messages = Vec::new();
    for offset in [5, 10, 260] {
        poll_at(&mut ctrl, &mut mem, t0, &[offset]);
        ctrl.drain_pending_msi_into(true, MSI.0, MSI.1, &mut messages);
        // The guest acknowledges BCIS; the platform drains after every access.
        write(&mut ctrl, &mut mem, REG_SD_STS, 1, u64::from(SDSTS_BCIS));
        ctrl.drain_pending_msi_into(true, MSI.0, MSI.1, &mut messages);
    }
    // The 250 ms poll crosses ten IOCs and sends one MSI.
    assert_eq!(messages.len(), 2);
    let poll = &ctrl.diagnostic_records()[0];
    assert!(
        poll.ends_with(" ioc_completions=11 max_iocs_per_poll=10 msis_sent=2"),
        "{poll}"
    );
}

#[test]
fn stream_record_reports_the_programmed_buffer_position_buffer_and_lpib_reads() {
    let entries = [(1_000, false), (PERIOD, false), (2_880, true)];
    let (mut ctrl, mut mem) = running_stream(&entries, true);
    poll_at(&mut ctrl, &mut mem, Instant::now(), &[0, 4]);
    // Reads that cover SD0 LPIB (0x84..0x88) count; neighbours do not.
    let reads = [(REG_SD_LPIB, 4), (REG_SD_LPIB + 2, 2), (REG_SD_CTL, 8)];
    for (offset, size) in reads.into_iter().chain([(REG_SD_CTL, 4), (REG_SD_CBL, 4)]) {
        ctrl.mmio_read(offset, size);
    }
    write(&mut ctrl, &mut mem, REG_SD_CTL, 3, 0);
    ctrl.mmio_read(REG_SD_LPIB, 4); // stopped: not a read during playback
    assert_eq!(
        ctrl.diagnostic_records()[1],
        "hda stream: runs=1 rate_hz=48000 channels=2 bits=16 cbl_bytes=5800 cbl_ms=30.208 \
         entries=3 entry_bytes_min=1000 entry_bytes_max=2880 entry_ms_min=5.208 \
         entry_ms_max=15.000 ioc_entries=1 position_buffer=1 lpib_reads=3 running_ms=4"
    );
}

#[test]
fn stream_record_counts_runs_and_keeps_the_latest_runs_shape() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    poll_at(&mut ctrl, &mut mem, t0, &[0, 10]);
    // Stopped and reset, the guest programs one IOC-flagged 20 ms entry.
    write(&mut ctrl, &mut mem, REG_SD_CTL, 3, 0);
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, u64::from(SDCTL_SRST));
    write(&mut ctrl, &mut mem, REG_SD_CTL, 1, 0);
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&PCM.to_le_bytes());
    descriptor[8..12].copy_from_slice(&(2 * PERIOD).to_le_bytes());
    descriptor[12..16].copy_from_slice(&BDL_IOC.to_le_bytes());
    assert!(mem.write_bytes(BDL, &descriptor));
    write(&mut ctrl, &mut mem, REG_SD_LVI, 2, 0);
    write(&mut ctrl, &mut mem, REG_SD_CBL, 4, u64::from(2 * PERIOD));
    write(&mut ctrl, &mut mem, REG_SD_CTL, 3, RUN_WITH_IOC);
    poll_at(&mut ctrl, &mut mem, t0, &[20, 25]);
    assert_eq!(
        ctrl.diagnostic_records()[1],
        "hda stream: runs=2 rate_hz=48000 channels=2 bits=16 cbl_bytes=3840 cbl_ms=20.000 \
         entries=1 entry_bytes_min=3840 entry_bytes_max=3840 entry_ms_min=20.000 \
         entry_ms_max=20.000 ioc_entries=1 position_buffer=0 lpib_reads=0 running_ms=15"
    );
}

#[test]
fn diagnostics_cover_the_controller_lifetime_across_guest_and_platform_resets() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, true);
    poll_at(&mut ctrl, &mut mem, Instant::now(), &[0, 10, 30]);
    let before = ctrl.diagnostic_records();
    assert!(before[0].starts_with("hda poll: polls=2 "), "{}", before[0]);
    write(&mut ctrl, &mut mem, REG_GCTL, 4, 0); // guest controller reset
    assert_eq!(ctrl.gctl, 0);
    assert_eq!(ctrl.diagnostic_records(), before);
    ctrl.reset_runtime_state(); // platform reset with the environment's sink
    assert_eq!(ctrl.diagnostic_records(), before);
    ctrl.set_pcm_sink(None); // an installed sink takes the controller-reset path
    ctrl.reset_runtime_state();
    assert_eq!(ctrl.diagnostic_records(), before);
}

#[test]
fn idle_records_are_zero_and_the_host_sinks_records_follow_them() {
    let mut ctrl = HdaController::with_pcm_output_path::<&Path>(None);
    assert_eq!(
        ctrl.diagnostic_records(),
        [
            "hda poll: polls=0 gap_le_10ms=0 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 \
             gap_gt_100ms=0 max_gap_ms=0 capped_polls=0 capped_lost_ms=0 ioc_completions=0 \
             max_iocs_per_poll=0 msis_sent=0",
            "hda stream: runs=0 rate_hz=0 channels=0 bits=0 cbl_bytes=0 cbl_ms=0.000 entries=0 \
             entry_bytes_min=0 entry_bytes_max=0 entry_ms_min=0.000 entry_ms_max=0.000 \
             ioc_entries=0 position_buffer=0 lpib_reads=0 running_ms=0",
            "hda repeat: passes=0 silent_passes=0 compared_passes=0 repeat_passes=0 \
             max_repeat_run=0",
        ]
    );
    let host = || vec!["hda CoreAudio timing: callbacks=0".to_string()];
    ctrl.set_diagnostic_host_records(Some(Box::new(host)));
    let records = ctrl.diagnostic_records();
    assert_eq!(records.len(), 4);
    assert_eq!(records[3], "hda CoreAudio timing: callbacks=0");
}
