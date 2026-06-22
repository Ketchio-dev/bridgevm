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
fn ep0_hid_set_protocol_boot_completes_no_data_setup_status_transfer() {
    // Given: the G022 C003 live trace sends HID SET_PROTOCOL(boot) as a no-data EP0 request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 0, 0, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup and status stage success events are posted and EP0 advances.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
}

#[test]
fn ep0_hid_set_protocol_report_completes_no_data_setup_status_transfer() {
    // Given: the HID interface also accepts report protocol as a no-data EP0 request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 1, 0, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup and status stage success events are posted and EP0 advances.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
}

#[test]
fn ep0_hid_set_protocol_rejects_unknown_protocol_value() {
    // Given: values outside boot/report protocol are not modeled for the HID keyboard.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 2, 0, 0);

    // When: the guest requests an unsupported protocol value.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no completion event is posted and EP0 stays on the setup TRB.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

#[test]
fn ep0_hid_set_protocol_rejects_nonzero_length() {
    // Given: HID SET_PROTOCOL is defined here as a no-data request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 0, 0, 1);

    // When: the setup packet claims a data stage exists.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the malformed request is rejected without moving EP0.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

#[test]
fn ep0_hid_set_protocol_rejects_nonzero_interface_index() {
    // Given: the modeled HID keyboard is exposed on interface 0.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 0, 1, 0);

    // When: the guest targets a different interface.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no completion event is posted and EP0 stays on the setup TRB.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

fn prepare_addressed_set_protocol(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    protocol: u16,
    interface_index: u16,
    length: u16,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_set_protocol_transfer(mem, protocol, interface_index, length);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_set_protocol_transfer(
    mem: &mut TestRam,
    protocol: u16,
    interface_index: u16,
    length: u16,
) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0b,
            value: protocol,
            index: interface_index,
            length,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}
