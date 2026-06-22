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
const STRING0_DESCRIPTOR: [u8; 4] = [4, 3, 0x09, 0x04];

#[test]
fn ep0_get_descriptor_string0_returns_langid_prefix() {
    // Given: the guest asks for the two-byte prefix of USB string descriptor zero.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_get_descriptor_transfer(&mut mem, EP0_RING, 2, 0x0300, true);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; STRING0_DESCRIPTOR.len()]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the deterministic LANGID descriptor prefix is returned with setup/data/status events.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 2).unwrap(), [4, 3]);
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER + 2, 2).unwrap(), [0xaa; 2]);
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
fn ep0_get_configuration_returns_current_configuration_after_set_configuration() {
    // Given: the guest has selected configuration 1 and then asks for the current value.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_set_configuration_transfer(&mut mem, EP0_RING);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    let get_configuration_ring = EP0_RING + (TRB_SIZE * 2);
    write_get_configuration_transfer(&mut mem, get_configuration_ring, true, 1);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 2]));

    // When: the guest rings slot 1 endpoint 0 for GET_CONFIGURATION.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: byte 1 is returned as the selected configuration value.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 2).unwrap(), [1, 0xaa]);
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        get_configuration_ring,
    );
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 4),
        get_configuration_ring + TRB_SIZE,
        0,
    );
    assert_success_transfer_event_with_residual(
        &mem,
        EVENT_RING + (TRB_SIZE * 5),
        get_configuration_ring + (TRB_SIZE * 2),
        0,
    );
    assert_eq!(
        xhci.slot1_ep0_dequeue,
        get_configuration_ring + (TRB_SIZE * 3)
    );
}

#[test]
fn ep0_hid_set_report_completes_out_data_control_transfer() {
    // Given: the HID interface receives SET_REPORT(Output) with one OUT data byte.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0x02]));
    write_hid_set_report_transfer(&mut mem, false, 1);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup, OUT data, and status completion events are posted and EP0 advances.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_success_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        EP0_RING + (TRB_SIZE * 2),
    );
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 3));
}

#[test]
fn ep0_get_configuration_returns_zero_after_controller_reset() {
    // Given: configuration 1 was selected before the host controller reset.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_set_configuration_transfer(&mut mem, EP0_RING);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    xhci.mmio_write(0x40, 4, 1 << 1);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_get_configuration_transfer(&mut mem, EP0_RING, true, 1);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa]));

    // When: the reset controller handles GET_CONFIGURATION.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the current configuration has returned to the unconfigured value.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 1).unwrap(), [0]);
}

#[test]
fn ep0_get_configuration_rejects_out_data_stage() {
    // Given: GET_CONFIGURATION is malformed with an OUT data stage.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_get_configuration_transfer(&mut mem, EP0_RING, false, 1);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the malformed direction is rejected without writing or moving EP0.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 1).unwrap(), [0xaa]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

#[test]
fn ep0_hid_set_report_rejects_in_data_stage() {
    // Given: HID SET_REPORT is malformed with an IN data stage.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(&mut xhci, &mut mem);
    write_hid_set_report_transfer(&mut mem, true, 1);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the malformed direction is rejected without writing or moving EP0.
    assert_eq!(mem.read_bytes(DATA_STAGE_BUFFER, 1).unwrap(), [0xaa]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

fn prepare_addressed_ep0(xhci: &mut XhciController, mem: &mut TestRam) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_get_descriptor_transfer(
    mem: &mut TestRam,
    ring: u64,
    length: u16,
    value: u16,
    in_data: bool,
) {
    write_setup(
        mem,
        ring,
        SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x06,
            value,
            index: 0,
            length,
        },
    );
    write_data_and_status(mem, ring, length, in_data);
}

fn write_get_configuration_transfer(mem: &mut TestRam, ring: u64, in_data: bool, length: u16) {
    write_setup(
        mem,
        ring,
        SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x08,
            value: 0,
            index: 0,
            length,
        },
    );
    write_data_and_status(mem, ring, length, in_data);
}

fn write_hid_set_report_transfer(mem: &mut TestRam, in_data: bool, length: u16) {
    write_setup(
        mem,
        EP0_RING,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length,
        },
    );
    write_data_and_status(mem, EP0_RING, length, in_data);
}

fn write_set_configuration_transfer(mem: &mut TestRam, ring: u64) {
    write_setup(
        mem,
        ring,
        SetupPacketFields {
            bm_request_type: 0x00,
            request: 0x09,
            value: 1,
            index: 0,
            length: 0,
        },
    );
    mem.write_u32(
        ring + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn write_setup(mem: &mut TestRam, ring: u64, fields: SetupPacketFields) {
    mem.write_u64(ring, setup_packet_parameter(fields));
    mem.write_u32(ring + 8, 8);
    mem.write_u32(ring + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
}

fn write_data_and_status(mem: &mut TestRam, ring: u64, length: u16, in_data: bool) {
    mem.write_u64(ring + TRB_SIZE, DATA_STAGE_BUFFER);
    mem.write_u32(ring + TRB_SIZE + 8, u32::from(length));
    let direction = if in_data {
        TRB_DATA_STAGE_DIRECTION_IN
    } else {
        0
    };
    mem.write_u32(
        ring + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_DATA_STAGE) | direction,
    );
    mem.write_u32(
        ring + (TRB_SIZE * 2) + 12,
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
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, 1);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & 1, 1);
}
