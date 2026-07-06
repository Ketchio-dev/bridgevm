use super::ports::{PORTSC_PP, PORT_REG_BASE, PORT_REG_STRIDE};
use super::test_support::TestRam;
use super::*;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PLS_RX_DETECT: u32 = 5 << 5;
const PORTSC_SPEED_HIGH: u32 = 3 << 10;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;
const USB_STS_CNR: u32 = 1 << 11;
const XHCI_EXT_CAP_SUPPORTED_PROTOCOL: u32 = 2;
const XHCI_EXT_CAP_USB_LEGACY_SUPPORT: u32 = 1;
const USBLEGSUP_BIOS_OWNED: u32 = 1 << 16;
const USBLEGSUP_OS_OWNED: u32 = 1 << 24;

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
    assert_eq!(xhci.mmio_read(0x28, 4), 0x0000_0401);
    assert_eq!(xhci.mmio_read(0x30, 4), 0x0300_0002);
    assert_eq!(xhci.mmio_read(0x34, 4), 0x2042_5355);
    assert_eq!(xhci.mmio_read(0x38, 4), 0x0000_0405);
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
            assert_eq!(portsc, PORTSC_PP | PORTSC_PLS_RX_DETECT);
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
fn supported_protocol_range_matches_connected_high_speed_root_port() {
    let xhci = XhciController::new();

    let usb2_protocol_ports = u32::try_from(xhci.mmio_read(0x28, 4)).unwrap();
    let compatible_port_offset = usb2_protocol_ports & 0xff;
    let compatible_port_count = (usb2_protocol_ports >> 8) & 0xff;
    let compatible_port_limit = compatible_port_offset + compatible_port_count;
    let connected_high_speed_root_port = 1;

    assert!(
        (compatible_port_offset..compatible_port_limit).contains(&connected_high_speed_root_port),
        "USB2 Supported Protocol compatible port range {}..={} must include connected high-speed root port {}",
        compatible_port_offset,
        compatible_port_limit.saturating_sub(1),
        connected_high_speed_root_port
    );
}

#[test]
fn linux_probe_register_sequence_handshakes_and_extended_caps_terminate() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x4000);

    assert_eq!(xhci.mmio_read(0x00, 1), u64::from(XHCI_CAP_LENGTH));
    assert_eq!(xhci.mmio_read(0x00, 4) >> 16, 0x0100);
    assert_ne!(xhci.mmio_read(0x04, 4) & 0xff, 0);
    assert_ne!((xhci.mmio_read(0x04, 4) >> 24) & 0xff, 0);

    xhci.mmio_write(0x40, 4, xhci.mmio_read(0x40, 4) & !u64::from(USB_CMD_RS));
    assert_poll_eq(&xhci, 0x44, USB_STS_HCH, USB_STS_HCH);

    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    assert_poll_eq(&xhci, 0x40, USB_CMD_HCRST, 0);
    assert_poll_eq(&xhci, 0x44, USB_STS_CNR, 0);

    let supported_protocol_caps = walk_linux_extended_caps(&mut xhci);
    assert!(supported_protocol_caps >= 1);

    mem.write_u64(0x1000, 0x3000);
    mem.write_u32(0x1008, 16);
    xhci.mmio_write(0x70, 8, 0x2000);
    xhci.mmio_write(0x58, 8, 0x3000 | 1);
    xhci.mmio_write(0x78, 4, 64);
    xhci.mmio_write(0x1024, 4, 0);
    xhci.mmio_write(0x1028, 4, 1);
    xhci.mmio_write(0x1030, 8, 0x1000);
    xhci.mmio_write(0x1038, 8, 0x3000);
    xhci.mmio_write(0x1020, 4, 0x2);

    assert_eq!(xhci.mmio_read(0x70, 8), 0x2000);
    assert_eq!(xhci.mmio_read(0x58, 8), 0x3000 | 1);
    assert_eq!(xhci.mmio_read(0x78, 4), 64);
    assert_eq!(xhci.mmio_read(0x1024, 4), 0);
    assert_eq!(xhci.mmio_read(0x1028, 4), 1);
    assert_eq!(xhci.mmio_read(0x1030, 8), 0x1000);
    assert_eq!(xhci.mmio_read(0x1038, 8), 0x3000);
    assert_eq!(xhci.mmio_read(0x1020, 4), 0x2);

    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_RS));
    assert_poll_eq(&xhci, 0x44, USB_STS_HCH, 0);
}

fn assert_poll_eq(xhci: &XhciController, offset: u64, mask: u32, expected: u32) {
    for _ in 0..16 {
        let value = xhci.mmio_read(offset, 4) as u32;
        if value & mask == expected {
            return;
        }
    }
    let value = xhci.mmio_read(offset, 4) as u32;
    assert_eq!(value & mask, expected);
}

fn walk_linux_extended_caps(xhci: &mut XhciController) -> usize {
    let hccparams1 = xhci.mmio_read(0x10, 4) as u32;
    let mut offset = u64::from(hccparams1 >> 16) << 2;
    let mut visited = Vec::new();
    let mut supported_protocol_caps = 0;

    for _ in 0..32 {
        if offset == 0 {
            return supported_protocol_caps;
        }
        assert!(visited.iter().all(|visited| *visited != offset));
        visited.push(offset);

        let header = xhci.mmio_read(offset, 4) as u32;
        let cap_id = header & 0xff;
        let next = (header >> 8) & 0xff;

        assert_ne!(cap_id, 0, "xECP pointed at an unimplemented zero header");
        match cap_id {
            XHCI_EXT_CAP_SUPPORTED_PROTOCOL => supported_protocol_caps += 1,
            XHCI_EXT_CAP_USB_LEGACY_SUPPORT => {
                xhci.mmio_write(offset, 4, u64::from(USBLEGSUP_OS_OWNED));
                let handoff = xhci.mmio_read(offset, 4) as u32;
                assert_eq!(handoff & USBLEGSUP_BIOS_OWNED, 0);
            }
            _ => {}
        }

        if next == 0 {
            return supported_protocol_caps;
        }
        offset += u64::from(next) << 2;
    }

    panic!("xECP walk did not terminate");
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
fn controller_reset_reannounces_connected_root_port_with_fresh_connect_change() {
    let mut xhci = XhciController::new();

    xhci.mmio_write(PORT_REG_BASE, 4, u64::from(PORTSC_CSC));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(
        portsc & (PORTSC_CCS | PORTSC_PP | PORTSC_SPEED_HIGH | PORTSC_CSC),
        PORTSC_CCS | PORTSC_PP | PORTSC_SPEED_HIGH | PORTSC_CSC
    );
    assert_eq!(portsc & (PORTSC_PED | PORTSC_PRC), 0);

    xhci.mmio_write(PORT_REG_BASE, 4, u64::from(PORTSC_PR));
    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(portsc & (PORTSC_PED | PORTSC_PRC), PORTSC_PED | PORTSC_PRC);
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
