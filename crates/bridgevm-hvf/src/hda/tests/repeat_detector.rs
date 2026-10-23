//! The stale-replay detector (repeat_detector.rs): a BDL-entry pass whose bytes
//! hash as on that entry's previous lap, while the stream runs, unless silent.

use super::super::*;
use super::diagnostics::{poll_at, running_stream, PCM, PERIOD, TWO_PERIODS};
use super::helpers::write;
use crate::{fwcfg::GuestMemoryMut, platform_virt::FlatGuestRam};
use std::time::Instant;

/// The guest writes `tag` over every byte of 10 ms BDL entry `index`.
fn fill(mem: &mut FlatGuestRam, index: u64, tag: u8) {
    let bytes = vec![tag; PERIOD as usize];
    assert!(mem.write_bytes(PCM + index * u64::from(PERIOD), &bytes));
}

fn repeat_record(ctrl: &HdaController) -> String {
    ctrl.diagnostic_records()[2].clone()
}

/// A primed two-entry stream; each 10 ms poll plays exactly one entry.
fn tagged_stream() -> (HdaController, FlatGuestRam, Instant) {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    fill(&mut mem, 0, 0x11);
    fill(&mut mem, 1, 0x22);
    let t0 = Instant::now();
    poll_at(&mut ctrl, &mut mem, t0, &[0, 10, 20]); // lap 1: nothing to compare yet
    (ctrl, mem, t0)
}

#[test]
fn a_tagged_lap_replayed_without_a_rewrite_counts_each_entry_as_a_repeat() {
    let (mut ctrl, mut mem, t0) = tagged_stream();
    poll_at(&mut ctrl, &mut mem, t0, &[30, 40]);
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=4 silent_passes=0 compared_passes=2 repeat_passes=2 max_repeat_run=2"
    );
}

#[test]
fn a_lap_the_guest_rewrote_is_not_a_repeat() {
    let (mut ctrl, mut mem, t0) = tagged_stream();
    fill(&mut mem, 0, 0x33);
    fill(&mut mem, 1, 0x44);
    poll_at(&mut ctrl, &mut mem, t0, &[30, 40]);
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=4 silent_passes=0 compared_passes=2 repeat_passes=0 max_repeat_run=0"
    );
}

#[test]
fn one_rewritten_byte_makes_a_fresh_pass() {
    let (mut ctrl, mut mem, t0) = tagged_stream();
    assert!(mem.write_bytes(PCM + u64::from(PERIOD) + 7, &[0x99]));
    poll_at(&mut ctrl, &mut mem, t0, &[30, 40]);
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=4 silent_passes=0 compared_passes=2 repeat_passes=1 max_repeat_run=1"
    );
}

#[test]
fn silence_replayed_lap_after_lap_is_never_a_repeat() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    poll_at(
        &mut ctrl,
        &mut mem,
        Instant::now(),
        &[0, 10, 20, 30, 40, 50, 60],
    );
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=6 silent_passes=6 compared_passes=0 repeat_passes=0 max_repeat_run=0"
    );
}

/// A1's stale read: 30 ms polls over a 20 ms buffer that the guest refills
/// after every poll replay the previous lap in one pass of every three.
#[test]
fn polls_longer_than_the_cyclic_buffer_replay_its_previous_lap() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    fill(&mut mem, 0, 1);
    fill(&mut mem, 1, 1);
    ctrl.poll(&mut mem, Some(t0));
    for (tag, offset) in [(2, 30), (3, 60), (4, 90)] {
        poll_at(&mut ctrl, &mut mem, t0, &[offset]);
        fill(&mut mem, 0, tag);
        fill(&mut mem, 1, tag);
    }
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=9 silent_passes=0 compared_passes=7 repeat_passes=3 max_repeat_run=1"
    );
}

#[test]
fn a_pass_the_dma_joined_mid_entry_is_not_compared() {
    let (mut ctrl, mut mem, t0) = tagged_stream();
    poll_at(&mut ctrl, &mut mem, t0, &[25]); // half of entry 0's second lap
    write(&mut ctrl, &mut mem, REG_SD_CTL, 3, 0);
    write(
        &mut ctrl,
        &mut mem,
        REG_SD_CTL,
        3,
        u64::from(SDCTL_IOCE | SDCTL_RUN),
    );
    // Resumed at the saved cursor: the rest of entry 0 is not a whole pass, and
    // a new run compares nothing with laps from before the pause.
    poll_at(&mut ctrl, &mut mem, t0, &[30, 35, 45, 55, 65]);
    assert_eq!(
        repeat_record(&ctrl),
        "hda repeat: passes=5 silent_passes=0 compared_passes=1 repeat_passes=1 max_repeat_run=1"
    );
}
