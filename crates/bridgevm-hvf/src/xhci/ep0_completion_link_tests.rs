use super::test_support::{
    assert_success_event_data_transfer_event, assert_success_transfer_event_for_trb,
    command_control, setup_command_rings_with_parameter, setup_packet_parameter, SetupPacketFields,
    TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING, TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
    TRB_TYPE_TRANSFER_EVENT,
};
use super::*;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const DATA_STAGE_BUFFER: u64 = 0x7000;
const EP0_COMPLETION_WRAP_TARGET: u64 = 0x8000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRB_IMMEDIATE_DATA: u32 = 1 << 6;
const LED_DATA_EVENT: u64 = 0xffff_dc0a_c7b7_f710;
const LED_STATUS_EVENT: u64 = 0xffff_dc0a_c7b7_f714;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const HID_POINTER_REPORT_DESCRIPTOR_LENGTH: u16 = 51;

#[test]
fn ep0_hid_pointer_report_descriptor_follows_link_trb_before_status_stage() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_pointer_report_descriptor_with_linked_completion(&mut xhci, &mut mem);

    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING + TRB_SIZE,
        0,
    );
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        EP0_COMPLETION_WRAP_TARGET,
    );
    assert_eq!(
        xhci.slot1_ep0_dequeue,
        EP0_COMPLETION_WRAP_TARGET + TRB_SIZE
    );
    assert_eq!(
        xhci.hid_semantic_stats()
            .hid_report_descriptor_gets_by_interface[1],
        1
    );
}

/// Windows' keyboard LED report: SET_REPORT with the byte in the data stage
/// (IDT) and an Event Data TRB after the data and the status stage. On every
/// boot after a VM's first, the one sent at shutdown crosses the ring's Link
/// TRB between the data stage's Event Data TRB and the status stage; left
/// pending, Windows halted and reset the controller and waited ~1.8 s.
#[test]
fn ep0_set_report_follows_link_trb_between_event_data_and_status_stage() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_led_report_crossing_link_after_event_data(&mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    assert_success_event_data_transfer_event(&mem, EVENT_RING + TRB_SIZE, LED_DATA_EVENT, 1);
    assert_success_event_data_transfer_event(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        LED_STATUS_EVENT,
        0,
    );
    assert_eq!(
        xhci.slot1_ep0_dequeue,
        EP0_COMPLETION_WRAP_TARGET + (TRB_SIZE * 2)
    );
    assert_eq!(xhci.hid_semantic_stats().hid_set_report, 1);
}

fn write_led_report_crossing_link_after_event_data(mem: &mut TestRam) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length: 1,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(
        EP0_RING + 12,
        transfer_control(TRB_TYPE_SETUP_STAGE) | TRB_IMMEDIATE_DATA | (2 << 16),
    );
    mem.write_u64(EP0_RING + TRB_SIZE, 0x02);
    mem.write_u32(EP0_RING + TRB_SIZE + 8, 1);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_IMMEDIATE_DATA | TRB_CHAIN,
    );
    mem.write_u64(EP0_RING + (TRB_SIZE * 2), LED_DATA_EVENT);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_EVENT_DATA) | TRB_IOC,
    );
    mem.write_u64(EP0_RING + (TRB_SIZE * 3), EP0_COMPLETION_WRAP_TARGET);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 3) + 12,
        transfer_control(TRB_TYPE_LINK) | TRB_LINK_TOGGLE_CYCLE,
    );
    // Past the Link TRB the producer cycle bit is toggled (0).
    mem.write_u32(
        EP0_COMPLETION_WRAP_TARGET + 12,
        (TRB_TYPE_STATUS_STAGE << 10) | TRB_CHAIN | TRB_DATA_STAGE_DIRECTION_IN,
    );
    mem.write_u64(EP0_COMPLETION_WRAP_TARGET + TRB_SIZE, LED_STATUS_EVENT);
    mem.write_u32(
        EP0_COMPLETION_WRAP_TARGET + TRB_SIZE + 12,
        (TRB_TYPE_EVENT_DATA << 10) | TRB_IOC,
    );
}

fn prepare_addressed_pointer_report_descriptor_with_linked_completion(
    xhci: &mut XhciController,
    mem: &mut TestRam,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_pointer_report_descriptor_with_linked_completion(mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_pointer_report_descriptor_with_linked_completion(mem: &mut TestRam) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x81,
            request: 0x06,
            value: 0x2200,
            index: 1,
            length: HID_POINTER_REPORT_DESCRIPTOR_LENGTH,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
    mem.write_u64(EP0_RING + TRB_SIZE, DATA_STAGE_BUFFER);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 8,
        u32::from(HID_POINTER_REPORT_DESCRIPTOR_LENGTH),
    );
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
    );
    mem.write_u64(EP0_RING + (TRB_SIZE * 2), EP0_COMPLETION_WRAP_TARGET);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_LINK),
    );
    mem.write_u32(
        EP0_COMPLETION_WRAP_TARGET + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}

fn assert_success_transfer_event_with_residual(
    mem: &TestRam,
    event_gpa: u64,
    trb_gpa: u64,
    residual: u32,
) {
    assert_eq!(mem.read_u64(event_gpa), trb_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, residual);
    let expected_completion_code = if residual > 0 { 13 } else { 1 };
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, expected_completion_code);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & 1, 1);
}
