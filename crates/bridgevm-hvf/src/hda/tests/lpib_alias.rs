//! The playback LPIB alias reads the stream position, as LPIB does. Playback
//! is SD1 since the capture stream took SD0, so its alias is 0x20a4; the
//! capture alias at 0x2084 is covered in capture.rs.
//!
//! The Windows guest reads the position only through the alias: live, with
//! playback at SD0, it read 0x2084 once per IOC and 0x84 never. While the alias read 0, its audio
//! engine wrote every 10 ms packet into the same entry of its two-entry cyclic
//! buffer, so every other 10 ms of guest playback reached the host as silence.

use super::super::*;
use super::diagnostics::{poll_at, running_stream, TWO_PERIODS};
use std::time::Instant;

#[test]
fn lpib_alias_reads_the_stream_position() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    let t0 = Instant::now();
    // A time base, 5 ms into entry 0, 5 ms into entry 1, then past the wrap.
    for (ms, lpib) in [(0, 0), (5, 960), (15, 2_880), (25, 960)] {
        poll_at(&mut ctrl, &mut mem, t0, &[ms]);
        assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), lpib);
        assert_eq!(ctrl.mmio_read(REG_SD_LPIB_ALIAS, 4), lpib, "at {ms} ms");
    }
}

/// The audio tick takes its time, waits for the platform, and can poll after
/// a vCPU that took a later time. Its older time must not become the base.
#[test]
fn a_poll_with_an_older_time_does_not_play_that_time_twice() {
    let (mut ctrl, mut mem) = running_stream(&TWO_PERIODS, false);
    poll_at(&mut ctrl, &mut mem, Instant::now(), &[0, 10, 5, 15]);
    // 15 ms of 192 bytes: 20 ms would have wrapped the 3,840-byte buffer to 0.
    assert_eq!(ctrl.mmio_read(REG_SD_LPIB, 4), 2_880);
}
