use super::configure_endpoint_tests::{
    setup_configure_endpoint_command, write_dci3_normal_trb, DCBAA, DCI3_ADD_CONTEXT_FLAG,
    DCI3_DWORD1, DCI3_DWORD4, EP_CONTEXT_BYTES, EP_CONTEXT_DWORD1_OFFSET, EP_CONTEXT_DWORD4_OFFSET,
    EP_TR_DEQUEUE_OFFSET, INPUT_CONTEXT, INPUT_CONTROL_ADD_CONTEXT_OFFSET,
    INPUT_CONTROL_DROP_CONTEXT_OFFSET, OUTPUT_CONTEXT, TRB_CYCLE, TRB_TYPE_TRANSFER_EVENT,
};
use super::test_support::{TestRam, DOORBELL_BASE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(super) const DCI5: u32 = 5;
const DCI5_ADD_CONTEXT_FLAG: u32 = 1 << DCI5;
const DCI5_INPUT_CONTEXT_OFFSET: u64 = 0xc0;
const DCI5_OUTPUT_CONTEXT_OFFSET: u64 = 0xa0;
pub(super) const DCI5_RING: u64 = 0x6400;
pub(super) const DCI5_BUFFER: u64 = 0x6a00;

#[test]
fn configure_endpoint_copies_pointer_dci5_context() {
    // Given: Configure Endpoint adds both keyboard DCI3 and pointer DCI5.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);

    // When: the command doorbell runs Configure Endpoint.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: DCI5 is published to the slot output context.
    assert_eq!(
        mem.read_bytes(
            OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET,
            EP_CONTEXT_BYTES
        )
        .unwrap(),
        mem.read_bytes(INPUT_CONTEXT + DCI5_INPUT_CONTEXT_OFFSET, EP_CONTEXT_BYTES)
            .unwrap()
    );
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING);
}

#[test]
fn configure_endpoint_without_dci5_add_context_does_not_arm_pointer_endpoint() {
    // Given: Configure Endpoint adds only the keyboard interrupt endpoint.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    mem.write_u64(DCBAA + 8, OUTPUT_CONTEXT);
    write_dci3_normal_trb(&mut mem, DCI5_RING, DCI5_BUFFER, true);

    // When: the command doorbell runs Configure Endpoint.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: DCI5 stays unarmed even if a pointer-like ring exists in memory.
    assert_eq!(xhci.slot1_dci5_dequeue, 0);
    assert_eq!(
        mem.read_bytes(
            OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET,
            EP_CONTEXT_BYTES
        )
        .unwrap(),
        [0; EP_CONTEXT_BYTES]
    );
}

#[test]
fn configure_endpoint_dci5_drop_context_clears_armed_pointer_endpoint() {
    // Given: pointer DCI5 was previously configured and armed.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING);
    assert_ne!(
        mem.read_u64(OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        0
    );

    // When: a later Configure Endpoint drops DCI5 without re-adding it.
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    mem.write_u32(
        INPUT_CONTEXT + INPUT_CONTROL_DROP_CONTEXT_OFFSET,
        DCI5_ADD_CONTEXT_FLAG,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: stale pointer state is not retained for future DCI5 doorbells.
    assert_eq!(xhci.slot1_dci5_dequeue, 0);
    assert_eq!(xhci.slot1_dci5_ring_base, 0);
    assert!(!xhci.slot1_dci5_dcs);
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET) & 0x7,
        0
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        0
    );
}

pub(super) fn setup_configure_endpoint_with_pointer(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_configure_endpoint_command(xhci, mem);
    mem.write_u64(DCBAA + 8, OUTPUT_CONTEXT);
    mem.write_u32(
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG | DCI5_ADD_CONTEXT_FLAG,
    );
    mem.write_u32(
        INPUT_CONTEXT + DCI5_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    mem.write_u64(
        INPUT_CONTEXT + DCI5_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI5_RING | TRB_CYCLE,
    );
    mem.write_u32(
        INPUT_CONTEXT + DCI5_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_dci3_normal_trb(mem, DCI5_RING, DCI5_BUFFER, true);
    assert!(mem.write_bytes(DCI5_BUFFER, &[0xaa; 8]));
}

pub(super) fn assert_short_packet_dci5_transfer_event(mem: &TestRam, event_gpa: u64, trb_gpa: u64) {
    assert_eq!(mem.read_u64(event_gpa), trb_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, 3);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, 13);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI5);
    assert_eq!(control & 1, 1);
}
