//! RTCLR readback and latched status follow Arm DDI 0224C §§3.3.3, 3.3.8.

use super::*;

#[test]
fn load_readback_starts_at_zero_independently_of_host_time() {
    for epoch in [0, 100, 0x1234_5678, u32::MAX] {
        let mut rtc = Pl031::new_at_epoch(epoch);
        assert_eq!(rtc.mmio_read(RTCLR, 4), 0);
    }
}

#[test]
fn load_readback_keeps_the_last_programmed_value() {
    let mut rtc = Pl031::new_at_epoch(100);
    for value in [0x1234_5678, u32::MAX, 17, 0] {
        rtc.mmio_write(RTCLR, 4, u64::from(value));
        assert_eq!(rtc.mmio_read(RTCLR, 4), u64::from(value));
        rtc.mmio_write(RTCMR, 4, 500);
        rtc.mmio_write(RTCIMSC, 4, 1);
        rtc.mmio_write(RTCICR, 4, 1);
        assert_eq!(rtc.mmio_read(RTCLR, 4), u64::from(value));
    }
}

fn latched_status_survives_load(masked: bool) {
    let mut rtc = Pl031::new_at_epoch(100);
    rtc.mmio_write(RTCMR, 4, 100);
    rtc.mmio_write(RTCIMSC, 4, u64::from(!masked));
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 1);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), u64::from(!masked));
    // Move below the match so a spurious clear cannot hide behind reassertion.
    rtc.mmio_write(RTCLR, 4, 10);
    assert_eq!(
        rtc.mmio_read(RTCRIS, 4),
        1,
        "loading time must not acknowledge an alarm"
    );
    assert_eq!(rtc.mmio_read(RTCMIS, 4), u64::from(!masked));
    rtc.mmio_write(RTCICR, 4, 0);
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 1);
    rtc.mmio_write(RTCIMSC, 4, 1);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), 1);
    rtc.mmio_write(RTCICR, 4, 1);
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 0);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), 0);
    assert_eq!(rtc.mmio_read(RTCMR, 4), 100);
}

#[test]
fn loading_time_preserves_an_unmasked_latched_alarm() {
    latched_status_survives_load(false);
}

#[test]
fn loading_time_preserves_a_masked_latched_alarm_until_acknowledged() {
    latched_status_survives_load(true);
}

#[test]
fn loading_before_a_future_match_does_not_create_an_alarm() {
    let mut rtc = Pl031::new_at_epoch(100);
    rtc.mmio_write(RTCMR, 4, 1000);
    rtc.mmio_write(RTCIMSC, 4, 1);
    rtc.mmio_write(RTCLR, 4, 10);
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 0);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), 0);
    assert_eq!(rtc.mmio_read(RTCMR, 4), 1000);
}
