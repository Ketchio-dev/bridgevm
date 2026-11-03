//! A guest late for an IOC finds the DMA 1 ms short of the entry it freed.
//!
//! Live, Windows serviced IOCs 9-20 ms late under host load; once the DMA had
//! reached the freed entry, the guest wrote the other one and the freed entry
//! replayed its previous 10 ms. Each test runs a 48 kHz stereo stream (192
//! bytes per ms) over two 10 ms entries and reads the cursor without counting
//! as a guest read; only `guest_reads_position` stands for the guest.

use super::super::*;
use super::diagnostics::{poll_at, running_stream, TWO_PERIODS};
use std::time::Instant;

fn guest_reads_position(ctrl: &HdaController) -> u64 {
    ctrl.mmio_read(REG_SD_LPIB_ALIAS, 4)
}

#[test]
fn a_late_guest_holds_the_dma_short_of_the_entry_its_ioc_freed() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    poll_at(&mut ctrl, &mut mem, t0, &[0, 5]);
    assert_eq!(guest_reads_position(&ctrl), 960);
    // Entry 0 completes at 10 ms; the guest does not read again. At 19 ms the
    // DMA is 1 ms short of entry 1's end and stays there through 25 ms.
    poll_at(&mut ctrl, &mut mem, t0, &[10, 19]);
    assert_eq!(ctrl.stream.lpib, 3_648);
    poll_at(&mut ctrl, &mut mem, t0, &[25]);
    assert_eq!((ctrl.stream.lpib, ctrl.hold.debt), (3_648, 1_152));
    // The late read finds entry 0 free; the DMA plays on and repays at 1.5x.
    assert_eq!(guest_reads_position(&ctrl), 3_648);
    poll_at(&mut ctrl, &mut mem, t0, &[26]);
    assert_eq!((ctrl.stream.lpib, ctrl.hold.debt), (96, 1_056));
}

#[test]
fn a_poll_longer_than_the_hold_plays_on_and_a_guest_that_never_reads_is_not_held() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    poll_at(&mut ctrl, &mut mem, t0, &[0, 5]);
    guest_reads_position(&ctrl);
    // A 21 ms poll from entry 1's guard: more than 10 ms is left at each guard
    // until the last, so it plays both entries' ends and holds only its rest.
    poll_at(&mut ctrl, &mut mem, t0, &[19, 40]);
    assert_eq!((ctrl.stream.lpib, ctrl.hold.debt), (3_648, 192));

    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    poll_at(&mut ctrl, &mut mem, t0, &[0, 5, 10, 19, 25]);
    assert_eq!((ctrl.stream.lpib, ctrl.hold.debt), (960, 0));
}
