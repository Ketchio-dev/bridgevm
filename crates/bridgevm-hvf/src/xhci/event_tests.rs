use super::event::USB_STS_EINT;
use super::test_support::{
    command_control, setup_command_rings, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ENABLE_SLOT,
};
use super::*;

#[test]
fn iman_write_one_to_clear_preserves_interrupt_enable() {
    // Given: an enabled interrupter with one pending Enable Slot completion event.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x3);

    // When: the guest acknowledges IP while keeping IE set.
    xhci.mmio_write(0x1020, 4, 0x3);

    // Then: IP is cleared, IE is preserved, and USBSTS.EINT is no longer reported.
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x2);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
}

#[test]
fn erdp_ehb_is_rw1c_busy_flag_not_stored_pointer_bit() {
    // Given: software initializes ERDP with EHB set as a write-one-to-clear value.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );

    // Then: the controller does not treat the guest's EHB write as a stored busy flag.
    assert_eq!(xhci.mmio_read(0x1038, 4) & 0x8, 0);

    // When: the controller posts a command completion event.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: the event makes the handler busy until software clears EHB via ERDP.
    assert_eq!(xhci.mmio_read(0x1038, 4) & 0x8, 0x8);
    xhci.mmio_write(0x1038, 8, (EVENT_RING + TRB_SIZE) | 0x8);
    assert_eq!(xhci.mmio_read(0x1038, 4) & 0x8, 0);
}

#[test]
fn erdp_ehb_write_consumes_pending_event_interrupt() {
    // Given: a command completion event has been posted to an enabled interrupter.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x3);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );

    // When: software advances ERDP and writes EHB=1 to acknowledge event handling.
    xhci.mmio_write(0x1038, 8, (EVENT_RING + TRB_SIZE) | 0x8);

    // Then: pending interrupt state clears, IE remains enabled, and EHB is not sticky.
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x2);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
    assert_eq!(xhci.mmio_read(0x1038, 4), EVENT_RING + TRB_SIZE);
}
