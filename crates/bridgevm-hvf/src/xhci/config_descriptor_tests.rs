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
const CONFIG_DESCRIPTOR: [u8; 34] = [
    9, 2, 34, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 1, 0x03, 0x01, 0x01, 0, 9, 0x21, 0x11, 0x01, 0, 1,
    0x22, 63, 0, 7, 5, 0x81, 0x03, 8, 0, 10,
];

#[test]
fn ep0_get_descriptor_configuration_returns_8_byte_header() {
    // Given: EDK2's UsbGetOneConfig first asks for the 8-byte configuration header.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_config_transfer(&mut xhci, &mut mem, 8, 0x0200);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; CONFIG_DESCRIPTOR.len()]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the header exposes the total length and reports setup/status completion events.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, 8).unwrap(),
        CONFIG_DESCRIPTOR[..8]
    );
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER + 8, CONFIG_DESCRIPTOR.len() - 8)
            .unwrap(),
        [0xaa; CONFIG_DESCRIPTOR.len() - 8]
    );
    assert_success_transfer_events_without_event_data(&mem);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

#[test]
fn ep0_get_descriptor_configuration_returns_full_descriptor_tree() {
    // Given: EDK2 follows the header with a full-length configuration descriptor request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_config_transfer(
        &mut xhci,
        &mut mem,
        u16::try_from(CONFIG_DESCRIPTOR.len()).unwrap(),
        0x0200,
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 48]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the whole HID keyboard-capable descriptor tree is copied.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, CONFIG_DESCRIPTOR.len())
            .unwrap(),
        CONFIG_DESCRIPTOR
    );
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER + CONFIG_DESCRIPTOR.len() as u64, 14)
            .unwrap(),
        [0xaa; 14]
    );
    assert_success_transfer_events_without_event_data(&mem);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

#[test]
fn ep0_get_descriptor_configuration_short_completes_overlength_request() {
    // Given: a host asks with extra room for the whole descriptor tree.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_config_transfer(&mut xhci, &mut mem, 255, 0x0200);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 255]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the controller returns a short descriptor and reports the residual.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, CONFIG_DESCRIPTOR.len())
            .unwrap(),
        CONFIG_DESCRIPTOR
    );
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER + CONFIG_DESCRIPTOR.len() as u64, 8)
            .unwrap(),
        [0xaa; 8]
    );
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING + (TRB_SIZE * 2),
        255 - u32::try_from(CONFIG_DESCRIPTOR.len()).unwrap(),
    );
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

#[test]
fn ep0_get_descriptor_unsupported_type_still_rejects_without_write() {
    // Given: the guest asks for an unsupported string descriptor.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_config_transfer(&mut xhci, &mut mem, 8, 0x0300);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 8]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: unsupported descriptors remain explicit no-write failures.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}

fn prepare_addressed_config_transfer(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    length: u16,
    value: u16,
) {
    write_ep0_input_context(mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_transfer(mem, length, value);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_ep0_input_context(mem: &mut TestRam, ep0_dequeue: u64) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, ep0_dequeue);
}

fn write_get_descriptor_transfer(mem: &mut TestRam, length: u16, value: u16) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x06,
            value,
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

fn assert_success_transfer_events_without_event_data(mem: &TestRam) {
    assert_success_transfer_event_for_trb(mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(
        mem,
        EVENT_RING + (TRB_SIZE * 2),
        EP0_RING + (TRB_SIZE * 2),
    );
}

fn assert_success_transfer_event_with_residual(
    mem: &TestRam,
    event_gpa: u64,
    trb_gpa: u64,
    residual: u32,
) {
    assert_eq!(mem.read_u64(event_gpa), trb_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, residual);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, 1);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & 1, 1);
}
