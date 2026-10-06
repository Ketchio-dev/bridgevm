use super::*;

#[test]
fn data_register_reports_epoch_seconds() {
    let mut rtc = Pl031::new_at_epoch(0x2026_0619);
    assert_eq!(rtc.mmio_read(RTCDR, 4), 0x2026_0619);
}

#[test]
fn load_register_resets_the_counter_base() {
    let mut rtc = Pl031::new_at_epoch(1);
    rtc.mmio_write(RTCLR, 4, 0x1234_5678);
    assert_eq!(rtc.mmio_read(RTCDR, 4), 0x1234_5678);
}

#[test]
fn match_interrupt_respects_mask_and_clear() {
    let mut rtc = Pl031::new_at_epoch(100);
    rtc.mmio_write(RTCMR, 4, 99);
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 1);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), 0);
    rtc.mmio_write(RTCIMSC, 4, 1);
    assert_eq!(rtc.mmio_read(RTCMIS, 4), 1);
    rtc.mmio_write(RTCICR, 4, 1);
    assert_eq!(rtc.mmio_read(RTCRIS, 4), 0);
}

#[test]
fn primecell_ids_match_arm_ddi_0224c() {
    let mut rtc = Pl031::new_at_epoch(0);
    assert_eq!(rtc.mmio_read(0xfe0, 4), 0x31);
    assert_eq!(rtc.mmio_read(0xfe4, 4), 0x10);
    assert_eq!(rtc.mmio_read(0xfe8, 4), 0x14);
    assert_eq!(rtc.mmio_read(0xff0, 4), 0x0d);
    assert_eq!(rtc.mmio_read(0xff4, 4), 0xf0);
    assert_eq!(rtc.mmio_read(0xff8, 4), 0x05);
    assert_eq!(rtc.mmio_read(0xffc, 4), 0xb1);
}
