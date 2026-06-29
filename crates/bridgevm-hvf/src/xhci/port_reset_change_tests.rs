use super::configure_endpoint_tests::*;
use super::ports::PORT_REG_BASE;
use super::test_support::{TestRam, DOORBELL_BASE, EVENT_RING, TRB_SIZE};
use super::*;

const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;
const POST_HCRST_ERST: u64 = 0x2000;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u32 = 34;
const USB_STS_EINT: u64 = 1 << 3;
const IMAN_INTERRUPT_PENDING: u64 = 1 << 0;
const IMAN_INTERRUPT_ENABLE: u64 = 1 << 1;

#[test]
fn port_reset_after_hcrst_posts_second_port_status_change_event() {
    // Given: a configured DCI3 endpoint was invalidated by HCRST before Windows reprograms events.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    assert_eq!(xhci.slot1_dci3_dequeue, 0);
    assert_eq!(xhci.slot1_dci3_ring_base, 0);
    assert_eq!(xhci.slot1_dci3_last_ring_base, 0);

    mem.write_u64(POST_HCRST_ERST, EVENT_RING);
    mem.write_u32(POST_HCRST_ERST + 8, 16);

    // When: the post-HCRST event ring is programmed through mem-backed MMIO.
    assert!(!xhci.mmio_write_with_mem(0x1020, 4, IMAN_INTERRUPT_ENABLE, &mut mem));
    assert!(!xhci.mmio_write_with_mem(0x1028, 4, 1, &mut mem));
    assert!(!xhci.mmio_write_with_mem(0x1038, 8, EVENT_RING | 0x8, &mut mem));
    assert!(xhci.mmio_write_with_mem(0x1030, 8, POST_HCRST_ERST, &mut mem));

    // Then: the first HCRST connect-change PSC lands at the first event slot.
    assert_eq!(mem.read_u64(EVENT_RING), 1 << 24);
    let first_control = mem.read_u32(EVENT_RING + 12);
    assert_eq!(
        (first_control >> 10) & 0x3f,
        TRB_TYPE_PORT_STATUS_CHANGE_EVENT
    );
    assert_eq!(first_control & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x1020, 4) & IMAN_INTERRUPT_PENDING,
        IMAN_INTERRUPT_PENDING
    );
    assert_eq!(xhci.mmio_read(0x44, 4) & USB_STS_EINT, USB_STS_EINT);

    // When: Windows consumes the first event, clears CSC, then writes PORTSC.PR.
    xhci.mmio_write_with_mem(0x1038, 8, EVENT_RING | 0x8, &mut mem);
    assert_eq!(xhci.mmio_read(0x1020, 4) & IMAN_INTERRUPT_PENDING, 0);
    assert_eq!(xhci.mmio_read(0x44, 4) & USB_STS_EINT, 0);
    assert!(!xhci.mmio_write_with_mem(PORT_REG_BASE, 4, u64::from(PORTSC_CSC), &mut mem));
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    let port_reset_posted =
        xhci.mmio_write_with_mem(PORT_REG_BASE, 4, u64::from(PORTSC_PR), &mut mem);

    // Then: port-reset completion raises PRC and posts a second PSC without restoring stale DCI3.
    assert!(port_reset_posted);
    let portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(portsc & (PORTSC_PED | PORTSC_PRC), PORTSC_PED | PORTSC_PRC);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 1 << 24);
    assert_eq!(mem.read_u32(EVENT_RING + TRB_SIZE + 8), 0);
    let second_control = mem.read_u32(EVENT_RING + TRB_SIZE + 12);
    assert_eq!(
        (second_control >> 10) & 0x3f,
        TRB_TYPE_PORT_STATUS_CHANGE_EVENT
    );
    assert_eq!(second_control & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x1020, 4) & IMAN_INTERRUPT_PENDING,
        IMAN_INTERRUPT_PENDING
    );
    assert_eq!(xhci.mmio_read(0x44, 4) & USB_STS_EINT, USB_STS_EINT);
    assert_eq!(xhci.slot1_dci3_dequeue, 0);
    assert_eq!(xhci.slot1_dci3_ring_base, 0);
    assert_eq!(xhci.slot1_dci3_last_ring_base, 0);
}
