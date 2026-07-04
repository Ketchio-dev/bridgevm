use super::configure_endpoint_tests::{
    assert_success_dci3_transfer_event, setup_configure_endpoint_command, write_dci3_normal_trb,
    DCI3, DCI3_BUFFER, DCI3_OUTPUT_CONTEXT_OFFSET, EP_TR_DEQUEUE_OFFSET, OUTPUT_CONTEXT, TRB_CYCLE,
};
use super::test_support::{
    command_control, setup_command_rings_with_parameter, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const NEW_DCI3_RING: u64 = 0x7600;
const NEW_DCI3_BUFFER: u64 = 0x7800;
const COMMAND_ENDPOINT_ID_SHIFT: u32 = 16;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;

#[test]
fn slot1_dci3_set_tr_dequeue_pointer_moves_interrupt_ring() {
    // Given: Configure Endpoint installed DCI3, then Windows moves it with Set TR Dequeue Pointer.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    write_dci3_normal_trb(&mut mem, NEW_DCI3_RING, NEW_DCI3_BUFFER, true);
    assert!(mem.write_bytes(NEW_DCI3_BUFFER, &[0xcc; 8]));
    setup_set_dci3_dequeue_pointer_command(&mut xhci, &mut mem, NEW_DCI3_RING | TRB_CYCLE);

    // When: the endpoint-3 dequeue update completes and DCI3 is polled.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, NEW_DCI3_RING);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        NEW_DCI3_RING | TRB_CYCLE
    );
    assert!(xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the interrupt report and transfer event are produced from the moved DCI3 ring.
    assert_eq!(
        mem.read_bytes(NEW_DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, NEW_DCI3_RING);
    assert_eq!(xhci.slot1_dci3_dequeue, NEW_DCI3_RING + TRB_SIZE);
}

fn setup_set_dci3_dequeue_pointer_command(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    dequeue: u64,
) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        dequeue,
        command_control(TRB_TYPE_SET_TR_DEQUEUE_POINTER, ENABLE_SLOT_ID)
            | (DCI3 << COMMAND_ENDPOINT_ID_SHIFT),
    );
}
