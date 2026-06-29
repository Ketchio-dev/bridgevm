use super::configure_endpoint_tests::*;
use super::ports::PORT_REG_BASE;
use super::test_support::{
    assert_success_completion, command_control, setup_command_rings,
    setup_command_rings_with_parameter, TestRam, CMD_RING, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE, TRB_TYPE_ENABLE_SLOT,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const PORTSC_CCS: u32 = 1 << 0;
const PORTSC_PED: u32 = 1 << 1;
const PORTSC_PR: u32 = 1 << 4;
const PORTSC_PP: u32 = 1 << 9;
const PORTSC_SPEED_HIGH: u32 = 3 << 10;
const PORTSC_CSC: u32 = 1 << 17;
const PORTSC_PRC: u32 = 1 << 21;
const FRESH_INPUT_CONTEXT: u64 = 0x7400;
const FRESH_OUTPUT_CONTEXT: u64 = 0x7c00;
const FRESH_EP0_RING: u64 = 0x8000;
const FRESH_DCI3_RING: u64 = 0x8200;
const FRESH_DCI3_BUFFER: u64 = 0x8600;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;
const POST_HCRST_ERST: u64 = 0x2000;
const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u32 = 34;
const USB_STS_EINT: u64 = 1 << 3;
const IMAN_INTERRUPT_PENDING: u64 = 1 << 0;
const IMAN_INTERRUPT_ENABLE: u64 = 1 << 1;

fn read_bytes(mem: &TestRam, gpa: u64, len: usize) -> Result<Vec<u8>, String> {
    mem.read_bytes(gpa, len)
        .ok_or_else(|| format!("unbacked test RAM read at {gpa:#x}"))
}

#[test]
fn queued_setup_input_after_hcrst_reacquires_only_fresh_dci3_output_dequeue() -> Result<(), String>
{
    // Given: DCI3 was configured before a real HCRST, leaving a stale diagnostic snapshot behind.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    xhci.slot1_dci3_last_dequeue = DCI3_RING;
    xhci.slot1_dci3_last_dcs = true;
    xhci.slot1_dci3_last_ring_base = DCI3_RING;
    xhci.slot1_dci3_last_ring_dcs = true;
    assert!(mem.write_bytes(DCI3_BUFFER, &[0xaa; 8]));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // When: queued setup-input drain runs before any post-reset DCI3 output dequeue exists.
    let drained = xhci.process_queued_dci3_input(&mut mem);

    // Then: the stale last snapshot is not used as a post-HCRST endpoint.
    assert!(!drained);
    assert_eq!(read_bytes(&mem, DCI3_BUFFER, 8)?, [0xaa; 8]);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 0);
    assert_eq!(stats.emitted_release_reports, 0);

    // When: Windows later provides a fresh DCI3 context after reset.
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert!(xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_RING | TRB_CYCLE
    );
    assert_eq!(mem.read_u64(DCI3_RING), DCI3_BUFFER);
    assert_eq!(mem.read_u32(DCI3_RING + 8), 8);
    assert_eq!(
        mem.read_u32(DCI3_RING + 12),
        (TRB_TYPE_NORMAL << 10) | TRB_CYCLE as u32
    );
    assert_eq!(mem.read_u64(DCI3_RING + TRB_SIZE), DCI3_WRAP_BUFFER);
    assert_eq!(xhci.setup_input_report_stats().queued_reports, 2);
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert!(xhci.process_queued_dci3_input(&mut mem));

    // Then: delayed setup-input drains from the fresh post-reset DCI3 endpoint.
    assert_eq!(
        read_bytes(&mem, DCI3_BUFFER, 8)?,
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_WRAP_BUFFER, 8)?, [0; 8]);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
    Ok(())
}

#[test]
fn post_hcrst_port_reset_allows_fresh_slot_lifecycle_to_publish_dci3() {
    // Given: slot 1 had a configured DCI3 endpoint before a host-controller reset.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);

    // When: Windows resets the host controller and starts root-port reset enumeration again.
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    let post_hcrst_portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;

    // Then: the HID port is connected and changed, but not stale-enabled before PORTSC.PR.
    assert_eq!(post_hcrst_portsc & PORTSC_PP, PORTSC_PP);
    assert_eq!(
        post_hcrst_portsc & (PORTSC_CCS | PORTSC_SPEED_HIGH | PORTSC_CSC),
        PORTSC_CCS | PORTSC_SPEED_HIGH | PORTSC_CSC
    );
    assert_eq!(post_hcrst_portsc & PORTSC_PED, 0);
    assert_eq!(post_hcrst_portsc & PORTSC_PRC, 0);
    assert_eq!(xhci.slot1_dci3_dequeue, 0);

    // When: the guest explicitly resets the connected port and runs a new slot lifecycle.
    xhci.mmio_write(PORT_REG_BASE, 4, u64::from(PORTSC_PR));
    let post_port_reset_portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(
        post_port_reset_portsc & (PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_PRC),
        PORTSC_CCS | PORTSC_PED | PORTSC_SPEED_HIGH | PORTSC_PRC
    );

    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);

    mem.write_u64(
        FRESH_INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        FRESH_EP0_RING | TRB_CYCLE,
    );
    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        FRESH_OUTPUT_CONTEXT,
    );
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        FRESH_INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);

    mem.write_u32(
        FRESH_INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    mem.write_u32(
        FRESH_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    mem.write_u64(
        FRESH_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        FRESH_DCI3_RING | TRB_CYCLE,
    );
    mem.write_u32(
        FRESH_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_dci3_normal_trb(&mut mem, FRESH_DCI3_RING, FRESH_DCI3_BUFFER, true);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        FRESH_INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: the post-reset lifecycle publishes a fresh DCI3 endpoint, not stale state.
    assert_eq!(xhci.slot1_dci3_dequeue, FRESH_DCI3_RING);
    assert_eq!(
        mem.read_u64(FRESH_OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        FRESH_DCI3_RING | TRB_CYCLE
    );
    assert_eq!(mem.read_u64(FRESH_DCI3_RING), FRESH_DCI3_BUFFER);
}

#[test]
fn port_status_change_event_posts_once_when_event_ring_is_programmed_after_hcrst() {
    // Given: a prior slot lifecycle left stale DCI3 state before HCRST.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);

    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    assert_eq!(xhci.slot1_dci3_dequeue, 0);
    assert_eq!(xhci.slot1_dci3_last_ring_base, 0);
    let post_hcrst_portsc = xhci.mmio_read(PORT_REG_BASE, 4) as u32;
    assert_eq!(
        post_hcrst_portsc & (PORTSC_CCS | PORTSC_SPEED_HIGH | PORTSC_CSC),
        PORTSC_CCS | PORTSC_SPEED_HIGH | PORTSC_CSC
    );

    mem.write_u64(POST_HCRST_ERST, EVENT_RING);
    mem.write_u32(POST_HCRST_ERST + 8, 16);

    // When: Windows programs a usable event ring through mem-backed MMIO.
    assert!(!xhci.mmio_write_with_mem(0x1020, 4, IMAN_INTERRUPT_ENABLE, &mut mem));
    assert!(!xhci.mmio_write_with_mem(0x1028, 4, 1, &mut mem));
    assert!(!xhci.mmio_write_with_mem(0x1038, 8, EVENT_RING | 0x8, &mut mem));
    let posted = xhci.mmio_write_with_mem(0x1030, 8, POST_HCRST_ERST, &mut mem);

    // Then: exactly one xHCI Port Status Change Event for port id 1 is posted.
    assert!(posted);
    assert_eq!((mem.read_u64(EVENT_RING) >> 24) & 0xff, 1);
    assert_eq!(mem.read_u32(EVENT_RING + 8), 0);
    let control = mem.read_u32(EVENT_RING + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_PORT_STATUS_CHANGE_EVENT);
    assert_eq!(control & 1, 1);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(
        xhci.mmio_read(0x1020, 4) & IMAN_INTERRUPT_PENDING,
        IMAN_INTERRUPT_PENDING
    );
    assert_eq!(xhci.mmio_read(0x44, 4) & USB_STS_EINT, USB_STS_EINT);
    assert_eq!(xhci.slot1_dci3_dequeue, 0);
    assert_eq!(xhci.slot1_dci3_last_ring_base, 0);

    let duplicate = xhci.mmio_write_with_mem(0x1028, 4, 1, &mut mem);
    assert!(!duplicate);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}
