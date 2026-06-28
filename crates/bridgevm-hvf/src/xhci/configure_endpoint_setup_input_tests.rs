use super::configure_endpoint_tests::*;
use super::test_support::{TestRam, DOORBELL_BASE, EVENT_RING, TRB_SIZE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

#[test]
fn slot1_dci3_doorbell_emits_queued_boot_key_report_once_then_releases() {
    // Given: Configure Endpoint installed DCI3 with two active interrupt IN buffers.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(!xhci.queue_boot_keyboard_space());

    // When: the guest polls DCI3 twice after the key was queued.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(!xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: Space is emitted exactly once and the next report releases the key.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));
}

#[test]
fn slot1_dci3_short_transfer_preserves_queued_setup_input_report_until_full_buffer() {
    // Given: Configure Endpoint installed DCI3 with a short interrupt IN buffer before full ones.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    mem.write_u32(DCI3_RING + 8, 4);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 2),
        DCI3_BUFFER + 0x40,
        true,
    );
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(mem.write_bytes(DCI3_BUFFER + 0x40, &[0xcc; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the guest first polls with a short buffer, then with two full buffers.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(!xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the short transfer does not expose or consume Space; the next full buffer gets it.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0, 0, 0xaa, 0xaa, 0xaa, 0xaa]
    );
    assert_eq!(
        mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x40, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
    assert_success_dci3_transfer_event(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        DCI3_RING + (TRB_SIZE * 2),
    );
}

#[test]
fn setup_input_action_queue_emits_minimal_navigation_sequence() {
    // Given: Configure Endpoint installed DCI3 with one interrupt IN buffer per setup action edge.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    for index in 0..6 {
        let trb = DCI3_RING + (TRB_SIZE * index);
        let buffer = DCI3_BUFFER + (0x20 * index);
        write_dci3_normal_trb(&mut mem, trb, buffer, true);
        assert!(mem.write_bytes(buffer, &[0xaa; 8]));
    }
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the minimal Windows Setup navigation sequence is queued and DCI3 is polled.
    assert_eq!(
        xhci.queue_setup_input_actions(&[
            SetupInputAction::Tab,
            SetupInputAction::Enter,
            SetupInputAction::Space,
        ]),
        Ok(())
    );
    for _ in 0..6 {
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    }

    // Then: each typed action emits one boot-keyboard report followed by release.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2b, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x20, 8).unwrap(), [0; 8]);
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER + 0x40, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x60, 8).unwrap(), [0; 8]);
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER + 0x80, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0xa0, 8).unwrap(), [0; 8]);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 6));
}

#[test]
fn slot1_dci3_failed_transfer_preserves_queued_boot_key_report() {
    // Given: a queued Space report and a first DCI3 Normal TRB with an unmapped buffer.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_INVALID_BUFFER, true);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the invalid transfer fails, then the same TRB is corrected and polled twice.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert!(!xhci.queue_boot_keyboard_space());
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_BUFFER, true);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the preserved Space report is emitted before the release report.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
}

#[test]
fn host_controller_reset_clears_captured_dci3_state() {
    // Given: Configure Endpoint installed slot 1 HID interrupt IN DCI3, then HCRST reset runs.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    // When: the guest rings the stale slot 1 DCI3 doorbell.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: no stale DCI3 transfer event is posted.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);

    // When: DCI3 is configured again after reset and the guest rings the doorbell.
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the pending key was cleared by HCRST, so the next report is still no-key.
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0; 8]);
}
