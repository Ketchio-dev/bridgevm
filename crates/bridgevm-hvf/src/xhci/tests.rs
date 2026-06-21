use super::*;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_SPEED_HIGH: u32 = 3 << 10;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;

const fn portsc_offset(port: usize) -> u64 {
    PORT_REG_BASE + port as u64 * PORT_REG_STRIDE
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
        let portsc = xhci.mmio_read(portsc_offset(port), 4) as u32;
        assert_eq!(portsc & PORTSC_PP, PORTSC_PP);
        if port != 0 {
            assert_eq!(portsc, PORTSC_PP);
        }
    }
}

#[test]
fn first_root_port_reports_connected_high_speed_keyboard_candidate() {
    let xhci = XhciController::new();

    let portsc = xhci.mmio_read(portsc_offset(0), 4) as u32;

    assert_eq!(portsc & PORTSC_PP, PORTSC_PP);
    assert_eq!(
        portsc & (PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_CSC),
        PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_CSC
    );
}

#[test]
fn portsc_writes_cannot_clear_power_without_ppc() {
    let mut xhci = XhciController::new();

    for port in 0..XHCI_PORT_COUNT {
        for value in [u64::from(PORTSC_PP), 0] {
            xhci.mmio_write(portsc_offset(port), 4, value);
            let portsc = xhci.mmio_read(portsc_offset(port), 4) as u32;
            assert_eq!(portsc & PORTSC_PP, PORTSC_PP);
        }
    }
}

#[test]
fn connected_port_change_bits_are_write_one_to_clear() {
    let mut xhci = XhciController::new();
    let port = portsc_offset(0);

    assert_ne!(xhci.mmio_read(port, 4) as u32 & PORTSC_CSC, 0);

    xhci.mmio_write(port, 4, u64::from(PORTSC_CSC));
    assert_eq!(xhci.mmio_read(port, 4) as u32 & PORTSC_CSC, 0);

    xhci.mmio_write(port, 4, u64::from(PORTSC_PR));
    assert_ne!(xhci.mmio_read(port, 4) as u32 & PORTSC_PRC, 0);

    xhci.mmio_write(port, 4, u64::from(PORTSC_PRC));
    assert_eq!(xhci.mmio_read(port, 4) as u32 & PORTSC_PRC, 0);
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
fn controller_reset_restores_initial_root_port_state() {
    let mut xhci = XhciController::new();

    xhci.mmio_write(PORT_REG_BASE, 4, u64::from(PORTSC_CSC));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(
        portsc & (PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_CSC),
        PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_CSC
    );
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
