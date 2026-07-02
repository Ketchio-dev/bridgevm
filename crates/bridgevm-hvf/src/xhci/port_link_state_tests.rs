use super::ports::{PORT_REG_BASE, PORT_REG_STRIDE};
use super::*;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PLS_MASK: u32 = 0xf << 5;
const PORTSC_PLS_U0: u32 = 0 << 5;
const PORTSC_PLS_RX_DETECT: u32 = 5 << 5;
const PORTSC_PLS_POLLING: u32 = 7 << 5;
const PORTSC_PP: u32 = 1 << 9;

#[test]
fn empty_port_reports_rx_detect_link_state() {
    // QEMU oracle: xhci_port_update leaves PLS=RxDetect(5) when no device is
    // attached, so an empty powered port reads PP | PLS=RxDetect.
    let xhci = XhciController::new();

    let portsc = xhci.mmio_read(PORT_REG_BASE + PORT_REG_STRIDE, 4) as u32;

    assert_eq!(portsc & PORTSC_CCS, 0);
    assert_eq!(portsc & PORTSC_PP, PORTSC_PP);
    assert_eq!(portsc & PORTSC_PLS_MASK, PORTSC_PLS_RX_DETECT);
}

#[test]
fn enabled_connected_port_reports_u0_link_state() {
    // The initial HID candidate port is connected and enabled; an enabled USB2
    // port has completed reset and links at U0.
    let xhci = XhciController::new();

    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;

    assert_eq!(portsc & (PORTSC_CCS | PORTSC_PED), PORTSC_CCS | PORTSC_PED);
    assert_eq!(portsc & PORTSC_PLS_MASK, PORTSC_PLS_U0);
}

#[test]
fn connected_port_awaiting_reset_reports_polling_link_state() {
    // QEMU oracle: a connected USB2 device that has not completed a port reset
    // links at Polling(7); PLS=U0 with PED=0 is an illegal USB2 combination
    // that makes the Windows bootmgr xHCI driver distrust the controller.
    let mut xhci = XhciController::new();

    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;

    assert_eq!(portsc & PORTSC_CCS, PORTSC_CCS);
    assert_eq!(portsc & PORTSC_PED, 0);
    assert_eq!(portsc & PORTSC_PLS_MASK, PORTSC_PLS_POLLING);
}

#[test]
fn port_reset_returns_link_state_to_u0() {
    let mut xhci = XhciController::new();
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    xhci.mmio_write(PORT_REG_BASE, 4, u64::from(PORTSC_PR));
    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;

    assert_eq!(portsc & PORTSC_PED, PORTSC_PED);
    assert_eq!(portsc & PORTSC_PLS_MASK, PORTSC_PLS_U0);
}
