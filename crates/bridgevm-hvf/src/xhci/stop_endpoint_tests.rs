use super::test_support::{
    assert_success_completion, command_control, setup_command_rings_with_parameter,
    setup_packet_parameter, SetupPacketFields, TestRam, CMD_RING, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;

const DCBAA: u64 = 0x4000;
const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const DATA_STAGE_BUFFER: u64 = 0x7000;
const OUTPUT_CONTEXT: u64 = 0x8000;
const DEVICE_DESCRIPTOR_LEN_U32: u32 = 18;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_OBSERVED_TYPE7: u32 = 7;
const TRB_TYPE_STOP_ENDPOINT: u32 = 15;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const EVENT_DATA_PARAMETER: u64 = 0xffff_b684_1bff_5790;
const SET_TR_EP0_DEQUEUE: u64 = 0x1111_df601;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
const EP_CONTEXT_DWORD0_OFFSET: u64 = 0x0;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_STATE_MASK: u32 = 0x7;
const EP_STATE_STOPPED: u32 = 3;

#[test]
fn stop_endpoint_after_ep0_event_data_td_publishes_stopped_output_context() {
    // Given: the live post-reinit sequence has Address Device, EP0 Event Data, then Stop Endpoint.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    write_ep0_input_context(&mut mem, EP0_RING | u64::from(TRB_CYCLE));
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u32(CMD_RING + TRB_SIZE + 12, stop_endpoint_control());
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_get_descriptor_device_transfer(&mut mem);

    // When: Address Device completes, GET_DESCRIPTOR(device) posts Event Data, then EP0 stops.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: Stop Endpoint publishes the current EP0 output state and post-TD dequeue.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD0_OFFSET)
            & EP_STATE_MASK,
        EP_STATE_STOPPED
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        EP0_RING + (TRB_SIZE * 4) | u64::from(TRB_CYCLE)
    );
    assert_success_completion(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        CMD_RING + TRB_SIZE,
        ENABLE_SLOT_ID,
    );
}

#[test]
fn set_tr_dequeue_pointer_after_ep0_stop_updates_output_context_dequeue() {
    // Given: the live sequence stops EP0 after an Event Data TD, then recovers EP0's dequeue.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    write_ep0_input_context(&mut mem, EP0_RING | u64::from(TRB_CYCLE));
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u32(CMD_RING + TRB_SIZE + 12, stop_endpoint_control());
    mem.write_u64(CMD_RING + (TRB_SIZE * 2), SET_TR_EP0_DEQUEUE);
    mem.write_u32(
        CMD_RING + (TRB_SIZE * 2) + 12,
        set_tr_dequeue_pointer_control(),
    );
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_get_descriptor_device_transfer(&mut mem);

    // When: Address Device, Event Data, Stop Endpoint, then Set TR Dequeue Pointer complete.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: the guest-visible EP0 output context uses the SetTRDequeue parameter.
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        SET_TR_EP0_DEQUEUE
    );
    assert_success_completion(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        CMD_RING + (TRB_SIZE * 2),
        ENABLE_SLOT_ID,
    );
}

fn write_ep0_input_context(mem: &mut TestRam, ep0_dequeue: u64) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, ep0_dequeue);
}

fn write_get_descriptor_device_transfer(mem: &mut TestRam) {
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
    mem.write_u32(EP0_RING + TRB_SIZE + 8, DEVICE_DESCRIPTOR_LEN_U32);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_DATA_STAGE) | TRB_DATA_STAGE_DIRECTION_IN,
    );
    mem.write_u64(EP0_RING + (TRB_SIZE * 2), EVENT_DATA_PARAMETER);
    mem.write_u32(EP0_RING + (TRB_SIZE * 2) + 8, 0x400000);
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 2) + 12,
        transfer_control(TRB_TYPE_OBSERVED_TYPE7) | (1 << 5),
    );
    mem.write_u32(
        EP0_RING + (TRB_SIZE * 3) + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}

fn stop_endpoint_control() -> u32 {
    command_control(TRB_TYPE_STOP_ENDPOINT, ENABLE_SLOT_ID) | (1 << 16)
}

fn set_tr_dequeue_pointer_control() -> u32 {
    command_control(TRB_TYPE_SET_TR_DEQUEUE_POINTER, ENABLE_SLOT_ID) | (1 << 16)
}
