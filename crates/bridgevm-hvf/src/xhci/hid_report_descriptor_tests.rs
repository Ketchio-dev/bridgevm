use super::test_support::{
    assert_success_transfer_event_for_trb, command_control, setup_command_rings_with_parameter,
    setup_packet_parameter, SetupPacketFields, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE, TRB_TYPE_TRANSFER_EVENT,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const DATA_STAGE_BUFFER: u64 = 0x7000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const HID_REPORT_DESCRIPTOR_LENGTH: u16 = 63;
const HID_REPORT_DESCRIPTOR_LENGTH_USIZE: usize = 63;
const HID_REPORT_DESCRIPTOR_OVERLENGTH: u16 = 255;
const HID_REPORT_DESCRIPTOR_OVERLENGTH_USIZE: usize = 255;
const HID_REPORT_DESCRIPTOR_TYPE: u8 = 0x22;

const BOOT_KEYBOARD_HID_REPORT_DESCRIPTOR: [u8; HID_REPORT_DESCRIPTOR_LENGTH_USIZE] = [
    0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x03, 0x95, 0x05, 0x75, 0x01,
    0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x03, 0x95, 0x06,
    0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xc0,
];

#[test]
fn ep0_get_descriptor_hid_report_returns_boot_keyboard_report_descriptor() {
    // Given: the HID interface asks EP0 for descriptor type 0x22, as advertised by config.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_hid_report_descriptor_transfer(
        &mut xhci,
        &mut mem,
        HID_REPORT_DESCRIPTOR_LENGTH,
    );
    assert!(mem.write_bytes(
        DATA_STAGE_BUFFER,
        &[0xaa; HID_REPORT_DESCRIPTOR_LENGTH_USIZE + 1],
    ));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: exactly the 63-byte boot keyboard report descriptor is returned.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, HID_REPORT_DESCRIPTOR_LENGTH_USIZE)
            .unwrap(),
        BOOT_KEYBOARD_HID_REPORT_DESCRIPTOR
    );
    assert_eq!(
        mem.read_bytes(
            DATA_STAGE_BUFFER + u64::from(HID_REPORT_DESCRIPTOR_LENGTH),
            1
        )
        .unwrap(),
        [0xaa]
    );
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING + TRB_SIZE,
        0,
    );
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        EP0_RING + (TRB_SIZE * 2),
        0,
    );
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

#[test]
fn ep0_get_descriptor_hid_report_short_completes_overlength_request() {
    // Given: the HID interface asks with extra room for the report descriptor.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_hid_report_descriptor_transfer(
        &mut xhci,
        &mut mem,
        HID_REPORT_DESCRIPTOR_OVERLENGTH,
    );
    assert!(mem.write_bytes(
        DATA_STAGE_BUFFER,
        &[0xaa; HID_REPORT_DESCRIPTOR_OVERLENGTH_USIZE],
    ));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the controller returns a short report descriptor and reports the residual.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, HID_REPORT_DESCRIPTOR_LENGTH_USIZE)
            .unwrap(),
        BOOT_KEYBOARD_HID_REPORT_DESCRIPTOR
    );
    assert_eq!(
        mem.read_bytes(
            DATA_STAGE_BUFFER + u64::from(HID_REPORT_DESCRIPTOR_LENGTH),
            8
        )
        .unwrap(),
        [0xaa; 8]
    );
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING + TRB_SIZE,
        u32::from(HID_REPORT_DESCRIPTOR_OVERLENGTH - HID_REPORT_DESCRIPTOR_LENGTH),
    );
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        EP0_RING + (TRB_SIZE * 2),
    );
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

fn prepare_addressed_hid_report_descriptor_transfer(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    length: u16,
) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_hid_report_descriptor_transfer(mem, length);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_hid_report_descriptor_transfer(mem: &mut TestRam, length: u16) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x81,
            request: 0x06,
            value: u16::from(HID_REPORT_DESCRIPTOR_TYPE) << 8,
            index: 0,
            length,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));

    mem.write_u64(EP0_RING + TRB_SIZE, DATA_STAGE_BUFFER);
    mem.write_u32(EP0_RING + TRB_SIZE + 8, u32::from(length));
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
