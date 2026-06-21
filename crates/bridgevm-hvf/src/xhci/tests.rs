use super::*;

const fn portsc_offset(port: u64) -> u64 {
    PORT_REG_BASE + port * PORT_REG_STRIDE
}

#[test]
fn reports_qemu_capability_and_extended_capability_registers() {
    let xhci = XhciController::new();

    assert_eq!(PORT_REG_BASE, u64::from(XHCI_CAP_LENGTH) + 0x400);
    assert_eq!(xhci.mmio_read(0x00, 1), u64::from(XHCI_CAP_LENGTH));
    assert_eq!(xhci.mmio_read(0x00, 4), 0x0100_0040);
    assert_eq!(xhci.mmio_read(0x04, 4), 0x0800_1040);
    assert_eq!(xhci.mmio_read(0x08, 4), 0x0000_000f);
    assert_eq!(xhci.mmio_read(0x10, 4), 0x0008_7001);
    assert_eq!(xhci.mmio_read(0x14, 4), 0x0000_2000);
    assert_eq!(xhci.mmio_read(0x18, 4), 0x0000_1000);
    assert_eq!(xhci.mmio_read(0x20, 4), 0x0200_0402);
    assert_eq!(xhci.mmio_read(0x24, 4), 0x2042_5355);
    assert_eq!(xhci.mmio_read(0x28, 4), 0x0000_0405);
    assert_eq!(xhci.mmio_read(0x30, 4), 0x0300_0002);
    assert_eq!(xhci.mmio_read(0x34, 4), 0x2042_5355);
    assert_eq!(xhci.mmio_read(0x38, 4), 0x0000_0401);
}

#[test]
fn operational_registers_are_benign_and_writable() {
    let mut xhci = XhciController::new();

    assert_eq!(xhci.mmio_read(0x44, 4), USB_STS_HCH.into());
    assert_eq!(xhci.mmio_read(0x48, 4), 1);

    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_RS | USB_CMD_HCRST));
    assert_eq!(xhci.mmio_read(0x40, 4), u64::from(USB_CMD_RS));
    assert_eq!(xhci.mmio_read(0x44, 4), 0);

    xhci.mmio_write(0x70, 8, 0x1234_5678_9abc_def0);
    xhci.mmio_write(0x78, 4, 8);
    assert_eq!(xhci.mmio_read(0x70, 8), 0x1234_5678_9abc_def0);
    assert_eq!(xhci.mmio_read(0x78, 4), 8);
}

#[test]
fn portsc_reports_hardwired_power_for_each_root_port() {
    let xhci = XhciController::new();

    for port in 0..XHCI_PORT_COUNT {
        assert_eq!(xhci.mmio_read(portsc_offset(port), 4), u64::from(PORTSC_PP));
    }
}

#[test]
fn portsc_writes_cannot_clear_power_without_ppc() {
    let mut xhci = XhciController::new();

    for port in 0..XHCI_PORT_COUNT {
        for value in [u64::from(PORTSC_PP), 0] {
            xhci.mmio_write(portsc_offset(port), 4, value);
            assert_eq!(xhci.mmio_read(portsc_offset(port), 4), u64::from(PORTSC_PP));
        }
    }
}

#[test]
fn port_companion_registers_and_unmodeled_ninth_port_read_zero() {
    let xhci = XhciController::new();

    for port in 0..XHCI_PORT_COUNT {
        let portsc = portsc_offset(port);

        assert_eq!(xhci.mmio_read(portsc + 0x4, 4), 0);
        assert_eq!(xhci.mmio_read(portsc + 0x8, 4), 0);
        assert_eq!(xhci.mmio_read(portsc + 0xc, 4), 0);
    }
    assert_eq!(xhci.mmio_read(portsc_offset(XHCI_PORT_COUNT), 4), 0);
}

#[test]
fn controller_reset_leaves_root_ports_powered_and_disconnected() {
    let mut xhci = XhciController::new();

    xhci.mmio_write(PORT_REG_BASE, 4, 0);
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    assert_eq!(xhci.mmio_read(PORT_REG_BASE, 4), u64::from(PORTSC_PP));
}

#[test]
fn msix_table_and_pba_are_bar_backed() {
    let mut xhci = XhciController::new();

    assert_eq!(xhci.mmio_read(u64::from(XHCI_MSIX_TABLE_OFFSET), 4), 0);
    xhci.mmio_write(u64::from(XHCI_MSIX_TABLE_OFFSET), 4, 0xfee0_0000);
    assert_eq!(
        xhci.mmio_read(u64::from(XHCI_MSIX_TABLE_OFFSET), 4),
        0xfee0_0000
    );
    assert_eq!(xhci.mmio_read(u64::from(XHCI_MSIX_PBA_OFFSET), 4), 0);
}
