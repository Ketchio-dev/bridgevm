use super::test_support::{
    command_control, setup_command_rings_with_parameter, setup_packet_parameter, SetupPacketFields,
    TestRam, CMD_RING, COMPLETION_CODE_SUCCESS, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE, TRB_TYPE_TRANSFER_EVENT,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::event::USB_STS_EINT;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const DATA_STAGE_BUFFER: u64 = 0x7000;
const EVENT_DATA_PARAMETER: u64 = 0xffff_b684_1bff_5790;
const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_OBSERVED_TYPE7: u32 = 7;
const TRB_EVENT_DATA: u32 = 1 << 2;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;

#[derive(Clone, Copy)]
enum CompletionShape {
    ObservedType7ThenStatus,
    MalformedIntermediateThenStatus(u32),
}

#[test]
fn address_device_command_captures_slot1_ep0_dequeue_from_input_context() {
    // Given: Address Device names an input context whose EP0 context points at a transfer ring.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_device_transfer(&mut mem);

    // When: the guest completes Address Device and then rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the slot doorbell consumed the transfer ring captured from the input context.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, DEVICE_DESCRIPTOR.len())
            .unwrap(),
        DEVICE_DESCRIPTOR
    );
}

#[test]
fn ep0_get_descriptor_device_writes_descriptor_and_posts_transfer_event() {
    // Given: slot 1 has an EP0 ring containing Setup/Data/type7/Status TRBs for GET_DESCRIPTOR.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_device_transfer(&mut mem);
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 32]));

    // When: the guest rings Address Device and then the slot 1 endpoint 0 doorbell.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: exactly the 18-byte device descriptor is written and a Transfer Event is posted.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, DEVICE_DESCRIPTOR.len())
            .unwrap(),
        DEVICE_DESCRIPTOR
    );
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER + 18, 14).unwrap(),
        [0xaa; 14]
    );
    assert_success_transfer_event(&mem, EVENT_RING + TRB_SIZE);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );
}

#[test]
fn ep0_get_descriptor_device_rejects_out_data_stage() {
    // Given: a GET_DESCRIPTOR transfer whose Data Stage points the wrong direction.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    write_ep0_input_context(&mut mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_device_transfer_with_data_control(
        &mut mem,
        transfer_control(TRB_TYPE_DATA_STAGE),
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));

    // When: the guest rings Address Device and then slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no descriptor is written for an OUT transfer.
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, DEVICE_DESCRIPTOR.len())
            .unwrap(),
        [0xaa; 18]
    );
}

#[test]
fn ep0_get_descriptor_device_rejects_malformed_intermediate_trb() {
    // Given: GET_DESCRIPTOR has an unsupported intermediate TRB before Status Stage.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(
        &mut xhci,
        &mut mem,
        CompletionShape::MalformedIntermediateThenStatus(6),
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));

    // When: the guest rings slot 1 endpoint 0.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the malformed transfer is rejected without a descriptor write or completion.
    assert_descriptor_buffer_is_unchanged(&mem, 0xaa);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}

#[test]
fn host_controller_reset_clears_captured_ep0_state() {
    // Given: Address Device captured EP0 state, then HCRST reset the controller.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(
        &mut xhci,
        &mut mem,
        CompletionShape::ObservedType7ThenStatus,
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    // When: the guest rings the stale slot 1 endpoint 0 doorbell.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the stale EP0 ring is not consumed.
    assert_descriptor_buffer_is_unchanged(&mem, 0xaa);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}

#[test]
fn wrong_slot_doorbell_does_not_consume_ep0_transfer() {
    // Given: Address Device captured a valid slot 1 EP0 ring.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(
        &mut xhci,
        &mut mem,
        CompletionShape::ObservedType7ThenStatus,
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));

    // When: the guest rings slot 2 endpoint 0.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 8, 4, 1, &mut mem));

    // Then: the doorbell is ignored without a descriptor write or completion.
    assert_descriptor_buffer_is_unchanged(&mem, 0xaa);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}

#[test]
fn wrong_target_doorbell_does_not_consume_ep0_transfer() {
    // Given: Address Device captured a valid slot 1 EP0 ring.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_ep0(
        &mut xhci,
        &mut mem,
        CompletionShape::ObservedType7ThenStatus,
    );
    assert!(mem.write_bytes(DATA_STAGE_BUFFER, &[0xaa; 18]));

    // When: the guest rings slot 1 endpoint 1.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 2, &mut mem));

    // Then: the doorbell is ignored without a descriptor write or completion.
    assert_descriptor_buffer_is_unchanged(&mem, 0xaa);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
}

fn write_ep0_input_context(mem: &mut TestRam, ep0_dequeue: u64) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, ep0_dequeue);
}

fn prepare_addressed_ep0(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    completion: CompletionShape,
) {
    write_ep0_input_context(mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_get_descriptor_device_transfer_with_completion(
        mem,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
        completion,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_get_descriptor_device_transfer(mem: &mut TestRam) {
    write_get_descriptor_device_transfer_with_data_control(
        mem,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
    );
}

fn write_get_descriptor_device_transfer_with_data_control(mem: &mut TestRam, data_control: u32) {
    write_get_descriptor_device_transfer_with_completion(
        mem,
        data_control,
        CompletionShape::ObservedType7ThenStatus,
    );
}

fn write_get_descriptor_device_transfer_with_completion(
    mem: &mut TestRam,
    data_control: u32,
    completion: CompletionShape,
) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x06,
            value: 0x0100,
            index: 0,
            length: 18,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));

    mem.write_u64(EP0_RING + TRB_SIZE, DATA_STAGE_BUFFER);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 8,
        u32::try_from(DEVICE_DESCRIPTOR.len()).unwrap(),
    );
    mem.write_u32(EP0_RING + TRB_SIZE + 12, data_control);

    match completion {
        CompletionShape::ObservedType7ThenStatus => {
            mem.write_u64(EP0_RING + (TRB_SIZE * 2), EVENT_DATA_PARAMETER);
            mem.write_u32(EP0_RING + (TRB_SIZE * 2) + 8, 0x400000);
            mem.write_u32(EP0_RING + (TRB_SIZE * 2) + 12, event_data_control());
        }
        CompletionShape::MalformedIntermediateThenStatus(trb_type) => {
            mem.write_u32(EP0_RING + (TRB_SIZE * 2) + 12, transfer_control(trb_type));
        }
    }

    mem.write_u32(
        EP0_RING + (TRB_SIZE * 3) + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}

fn event_data_control() -> u32 {
    transfer_control(TRB_TYPE_OBSERVED_TYPE7) | (1 << 5)
}

fn assert_success_transfer_event(mem: &TestRam, event_gpa: u64) {
    assert_eq!(mem.read_u64(event_gpa), EVENT_DATA_PARAMETER);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, 0);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, 1);
    assert_eq!((control >> 24) & 0xff, 1);
    assert_eq!(control & TRB_EVENT_DATA, TRB_EVENT_DATA);
    assert_eq!(control & 1, 1);
    assert_ne!(mem.read_u64(event_gpa), CMD_RING);
}

fn assert_descriptor_buffer_is_unchanged(mem: &TestRam, fill: u8) {
    assert_eq!(
        mem.read_bytes(DATA_STAGE_BUFFER, DEVICE_DESCRIPTOR.len())
            .unwrap(),
        [fill; 18]
    );
}
