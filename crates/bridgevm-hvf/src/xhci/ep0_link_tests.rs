use super::test_support::{
    assert_success_transfer_event_for_trb, command_control, setup_command_rings_with_parameter,
    setup_packet_parameter, SetupPacketFields, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const EP0_RING_WRAP_TARGET: u64 = 0x7000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_LINK: u32 = 6;

#[test]
fn ep0_hid_set_idle_follows_link_trb_after_status_stage() {
    // Given: Windows can place the next EP0 TD in a wrapped control-ring
    // segment after a Link TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_set_idle_transfer_at(&mut mem, EP0_RING, 1);
    mem.write_u64(EP0_RING + (TRB_SIZE * 2), EP0_RING_WRAP_TARGET);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_LINK),
    );
    write_set_idle_transfer_at(&mut mem, EP0_RING_WRAP_TARGET, 1);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the guest rings EP0 for the first TD at the end of the segment.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: EP0 is advanced to the Link TRB target, not left pointing at the
    // Link TRB itself, so the next doorbell reads a Setup Stage TRB.
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING_WRAP_TARGET);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), EP0_RING_WRAP_TARGET);
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 4),
        EP0_RING_WRAP_TARGET + TRB_SIZE,
    );
}

#[test]
fn ep0_hid_set_idle_follows_link_trb_at_current_dequeue() {
    // Given: after reset/re-address churn, Windows can leave the controller's
    // current EP0 dequeue parked on the segment Link TRB itself.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u64(EP0_RING, EP0_RING_WRAP_TARGET);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_LINK));
    write_set_idle_transfer_at(&mut mem, EP0_RING_WRAP_TARGET, 1);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);

    // When: the guest rings EP0 while the modeled dequeue points at the Link TRB.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup decoding follows the Link target instead of rejecting the
    // Link TRB as unexpected_setup_trb_type=0x6.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING_WRAP_TARGET);
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING_WRAP_TARGET + TRB_SIZE,
    );
    assert_eq!(
        xhci.slot1_ep0_dequeue,
        EP0_RING_WRAP_TARGET + (TRB_SIZE * 2)
    );
    assert_eq!(xhci.hid_semantic_stats().hid_set_idle_by_interface[1], 1);
}

#[test]
fn ep0_hid_set_idle_rejects_unreadable_current_link_target() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0_link_dequeue(&mut xhci, &mut mem, 0x9000);

    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
    assert_eq!(xhci.hid_semantic_stats().total_setup_packets, 0);
}

#[test]
fn ep0_hid_set_idle_rejects_non_setup_current_link_target() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0_link_dequeue(&mut xhci, &mut mem, EP0_RING_WRAP_TARGET);
    mem.write_u32(
        EP0_RING_WRAP_TARGET + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );

    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
    assert_eq!(xhci.hid_semantic_stats().total_setup_packets, 0);
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}

fn write_set_idle_transfer_at(mem: &mut TestRam, ring: u64, interface_index: u16) {
    mem.write_u64(
        ring,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0a,
            value: 0x0000,
            index: interface_index,
            length: 0,
        }),
    );
    mem.write_u32(ring + 8, 8);
    mem.write_u32(ring + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
    mem.write_u32(
        ring + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn prepare_addressed_ep0_link_dequeue(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    link_target: u64,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u64(EP0_RING, link_target);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_LINK));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}
