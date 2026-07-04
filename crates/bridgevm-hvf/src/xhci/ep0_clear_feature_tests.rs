use super::test_support::{
    assert_success_transfer_event_for_trb, command_control, setup_command_rings_with_parameter,
    setup_packet_parameter, SetupPacketFields, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_STATUS_STAGE: u32 = 4;

#[test]
fn ep0_clear_endpoint_halt_completes_no_data_setup_status_transfer() {
    // Given: Windows sends CLEAR_FEATURE(ENDPOINT_HALT) on the interrupt-IN
    // endpoint 0x81 (standard, endpoint recipient, no data) observed in the
    // live trace; previously STALLed.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_clear_feature_endpoint_halt(&mut xhci, &mut mem, 0x0081);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the request completes with setup + status events, so Windows does
    // not treat the interrupt endpoint as faulty.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
}

#[test]
fn ep0_clear_feature_on_other_endpoint_is_not_treated_as_endpoint_halt() {
    // Given: the model supports the observed interrupt-IN endpoint only.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_clear_feature_endpoint_halt(&mut xhci, &mut mem, 0x0083);

    // When: Windows asks to clear halt on an unsupported endpoint address.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no completion event is posted and EP0 stays on the setup TRB.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

fn prepare_addressed_clear_feature_endpoint_halt(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    endpoint_address: u16,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x02,
            request: 0x01,
            value: 0x0000,
            index: endpoint_address,
            length: 0,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}
