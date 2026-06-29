use super::configure_endpoint_tests::*;
use super::test_support::{
    command_control, setup_command_rings_with_parameter, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_STOP_ENDPOINT: u32 = 15;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const ENDPOINT_ID_EP0: u32 = 1;
const EP0_RECOVERY_RING: u64 = 0x7200;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
const EP_STATE_MASK: u32 = 0x7;
const EP_STATE_STOPPED: u32 = 3;
const FRESH_INPUT_CONTEXT: u64 = 0x7400;
const FRESH_OUTPUT_CONTEXT: u64 = 0x7c00;
const FRESH_DCI3_RING: u64 = 0x8000;
const FRESH_DCI3_BUFFER: u64 = 0x8400;
const SET_TR_DEQUEUE_POINTER_CONTROL: u32 =
    (ENABLE_SLOT_ID << 24) | (ENDPOINT_ID_EP0 << 16) | (TRB_TYPE_SET_TR_DEQUEUE_POINTER << 10) | 1;

#[test]
fn queued_setup_input_after_disable_slot_waits_for_fresh_dci3_context() {
    // Given: slot 1 has DCI3 configured, then EP0 is stopped and moved by Set TR Dequeue.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    setup_set_tr_dequeue_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    setup_stop_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RECOVERY_RING);
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET) & EP_STATE_MASK,
        EP_STATE_STOPPED
    );
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // When: firmware disables slot 1 before a fresh Configure Endpoint arrives.
    setup_disable_slot_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    let stale_drained = xhci.process_queued_dci3_input(&mut mem);

    // Then: stale DCI3 and EP0 state from the disabled slot are not reusable.
    assert!(
        !stale_drained,
        "DisableSlot left stale DCI3 reusable: dequeue={:#x} ring_base={:#x} last_dequeue={:#x}",
        xhci.slot1_dci3_dequeue, xhci.slot1_dci3_ring_base, xhci.slot1_dci3_last_dequeue
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(xhci.slot1_ep0_dequeue, 0);
    assert!(!xhci.slot1_ep0_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        0
    );

    // When: a fresh slot lifecycle publishes a new DCI3 endpoint context.
    setup_fresh_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    let fresh_drained = xhci.process_queued_dci3_input(&mut mem);

    // Then: the queued setup input drains only through the fresh DCI3 context.
    assert!(fresh_drained);
    assert_eq!(
        mem.read_bytes(FRESH_DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, FRESH_DCI3_RING);
}

fn setup_set_tr_dequeue_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        EP0_RECOVERY_RING | TRB_CYCLE,
        SET_TR_DEQUEUE_POINTER_CONTROL,
    );
}

fn setup_disable_slot_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        0,
        command_control(TRB_TYPE_DISABLE_SLOT, ENABLE_SLOT_ID),
    );
}

fn setup_stop_endpoint_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        0,
        command_control(TRB_TYPE_STOP_ENDPOINT, ENABLE_SLOT_ID) | (ENDPOINT_ID_EP0 << 16),
    );
}

fn setup_fresh_configure_endpoint_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        FRESH_INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        FRESH_OUTPUT_CONTEXT,
    );
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
    write_dci3_normal_trb(mem, FRESH_DCI3_RING, FRESH_DCI3_BUFFER, true);
    assert!(mem.write_bytes(FRESH_DCI3_BUFFER, &[0xcc; 8]));
}
