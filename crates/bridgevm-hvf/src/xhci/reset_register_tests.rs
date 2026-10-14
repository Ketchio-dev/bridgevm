use super::*;

#[test]
fn controller_reset_clears_device_notification_control_after_stop() {
    let mut xhci = XhciController::new();

    // FUNCTION_WAKE is the normal notification enabled by a USB3 host driver.
    assert_eq!(xhci.mmio_read(0x54, 4), 0);
    for notifications in [0x2, 0x8002] {
        xhci.mmio_write(0x54, 4, notifications);
        xhci.mmio_write(0x40, 4, u64::from(USB_CMD_RS));
        assert_eq!(xhci.mmio_read(0x54, 4), notifications);

        // Stopping preserves programming; HCRST restores the DNCTRL default.
        xhci.mmio_write(0x40, 4, 0);
        assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_HCH), 1);
        assert_eq!(xhci.mmio_read(0x54, 4), notifications);
        xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
        assert_eq!(xhci.mmio_read(0x40, 4) & u64::from(USB_CMD_HCRST), 0);
        assert_eq!(xhci.mmio_read(0x54, 4), 0);
    }
}
