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
fn ep0_hid_set_protocol_accepts_pointer_interface_index() {
    // Given: the pointer HID interface accepts boot/report protocol requests on interface 1.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_protocol(&mut xhci, &mut mem, 0, 1, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup and status stage success events are posted and EP0 advances.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
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

const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const SET_IDLE_EVENT_DATA_PARAMETER: u64 = 0xffff_868b_5719_e554;

#[test]
fn ep0_hid_set_idle_completes_no_data_setup_status_transfer() {
    // Given: Windows sends HID SET_IDLE (bRequest 0x0A, class-interface OUT,
    // no data) during keyboard enumeration; value = duration<<8 | reportId.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_idle(&mut xhci, &mut mem, 0x0000, 0, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup and status stage success events are posted and EP0 advances,
    // so Windows does NOT reset/re-enumerate the keyboard.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
}

#[test]
fn ep0_hid_set_idle_on_pointer_interface_does_not_arm_dci5_endpoint() {
    // Given: Windows can send SET_IDLE to the pointer HID interface before it
    // has configured or polled the pointer interrupt endpoint.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_idle(&mut xhci, &mut mem, 0x0000, 1, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the class request succeeds and is attributed to interface 1, but
    // DCI5 remains unarmed until Configure Endpoint or Set TR Dequeue Pointer.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
    assert_eq!(xhci.slot1_dci5_dequeue, 0);
    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.hid_set_idle, 1);
    assert_eq!(stats.hid_set_idle_by_interface[0], 0);
    assert_eq!(stats.hid_set_idle_by_interface[1], 1);
}

#[test]
fn ep0_hid_set_idle_completes_chained_event_data_status_transfer() {
    // Given: the observed Windows SET_IDLE TD is Setup, Status(Chain), Event
    // Data(IOC) carrying a URB cookie — the exact shape from the live trace.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0a,
            value: 0x0000,
            index: 0,
            length: 0,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE) | TRB_CHAIN,
    );
    mem.write_u64(EP0_RING + (TRB_SIZE * 2), SET_IDLE_EVENT_DATA_PARAMETER);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_EVENT_DATA) | TRB_IOC,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the transfer completes — setup and status stage events plus the
    // trailing Event Data event carrying the URB cookie — and EP0 advances
    // past the Event Data TRB, so Windows sees success and does not reset.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(
        mem.read_u64(EVENT_RING + (TRB_SIZE * 3)),
        SET_IDLE_EVENT_DATA_PARAMETER
    );
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

fn prepare_addressed_set_idle(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    value: u16,
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
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0a,
            value,
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
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}
