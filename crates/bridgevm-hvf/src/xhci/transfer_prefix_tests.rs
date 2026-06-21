use super::test_support::{
    command_control, setup_command_rings_with_parameter, setup_packet_parameter, SetupPacketFields,
    TestRam, COMPLETION_CODE_SUCCESS, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING, TRB_SIZE,
    TRB_TYPE_ADDRESS_DEVICE, TRB_TYPE_TRANSFER_EVENT,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const DATA_STAGE_BUFFER: u64 = 0x7000;
const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const TRB_EVENT_DATA: u32 = 1 << 2;

#[test]
fn ep0_get_descriptor_device_accepts_8_byte_prefix_without_event_data() {
    // Given: slot 1 has an EP0 GET_DESCRIPTOR(Device) ring requesting the first 8 bytes.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_device_prefix_transfer(&mut mem);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));

    // When: the guest rings Address Device and then slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: only the requested descriptor prefix is written and status TRB completion is reported.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, 8).unwrap(),
        DEVICE_DESCRIPTOR[..8]
    );
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER + 8, 10).unwrap(),
        [0xaa; 10]
    );
    assert_success_transfer_event_without_event_data(&mem, EVENT_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

fn write_ep0_input_context(mem: &mut TestRam, ep0_dequeue: u64) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, ep0_dequeue);
}

fn write_get_descriptor_device_prefix_transfer(mem: &mut TestRam) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x06,
            value: 0x0100,
            index: 0,
            length: 8,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));

    mem.write_u64(EP0_RING + TRB_SIZE, DATA_STAGE_BUFFER);
    mem.write_u32(EP0_RING + TRB_SIZE + 8, 8);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
    );

    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}

fn assert_success_transfer_event_without_event_data(mem: &TestRam, event_gpa: u64) {
    assert_eq!(mem.read_u64(event_gpa), EP0_RING + (TRB_SIZE * 2));
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, 0);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & TRB_EVENT_DATA, 0);
    assert_eq!(control & 1, 1);
}
