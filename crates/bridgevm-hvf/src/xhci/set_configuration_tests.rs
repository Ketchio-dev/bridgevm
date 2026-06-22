use super::test_support::{
    assert_success_transfer_event_for_trb, command_control, setup_command_rings_with_parameter,
    setup_packet_parameter, SetupPacketFields, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;
use crate::xhci::event::USB_STS_EINT;

const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const TRB_CYCLE: u32 = 1;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_STATUS_STAGE: u32 = 4;

#[test]
fn ep0_set_configuration_completes_no_data_setup_status_transfer() {
    // Given: EDK2 sends USB_REQ_SET_CONFIG(1) as a no-data control transfer.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_configuration(&mut xhci, &mut mem, 1, 0);

    // When: the guest rings slot 1 endpoint 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: setup and status stage success events are posted and EP0 advances.
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 2), EP0_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING + (TRB_SIZE * 2));
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );
}

#[test]
fn ep0_set_configuration_rejects_unsupported_configuration_value() {
    // Given: only configuration value 1 exists in the minimal descriptor tree.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_configuration(&mut xhci, &mut mem, 2, 0);

    // When: the guest asks for an unsupported configuration.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no SET_CONFIGURATION completion events are posted.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

#[test]
fn ep0_set_configuration_rejects_malformed_nonzero_length() {
    // Given: SET_CONFIGURATION is defined here as a no-data request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_configuration(&mut xhci, &mut mem, 1, 1);

    // When: the setup packet claims a data stage exists.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the malformed request is rejected without moving EP0.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

#[test]
fn ep0_set_configuration_rejects_missing_status_stage() {
    // Given: SET_CONFIGURATION omits the required status stage TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    prepare_addressed_set_configuration_without_status(&mut xhci, &mut mem);

    // When: the guest asks endpoint 0 to process the malformed transfer.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: no completion event is posted and EP0 stays on the setup TRB.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RING);
}

fn prepare_addressed_set_configuration(
    xhci: &mut XhciController,
    mem: &mut TestRam,
    configuration_value: u16,
    length: u16,
) {
    write_ep0_input_context(mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_set_configuration_transfer(mem, configuration_value, length);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn prepare_addressed_set_configuration_without_status(
    xhci: &mut XhciController,
    mem: &mut TestRam,
) {
    write_ep0_input_context(mem, EP0_RING | 1);
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    write_set_configuration_setup(mem, 1, 0);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, mem));
}

fn write_ep0_input_context(mem: &mut TestRam, ep0_dequeue: u64) {
    mem.write_u64(INPUT_CONTEXT + 0x40 + 8, ep0_dequeue);
}

fn write_set_configuration_transfer(mem: &mut TestRam, configuration_value: u16, length: u16) {
    write_set_configuration_setup(mem, configuration_value, length);
    mem.write_u32(
        EP0_RING + TRB_SIZE + 12,
        transfer_control(TRB_TYPE_STATUS_STAGE),
    );
}

fn write_set_configuration_setup(mem: &mut TestRam, configuration_value: u16, length: u16) {
    mem.write_u64(
        EP0_RING,
        setup_packet_parameter(SetupPacketFields {
            bm_request_type: 0x00,
            request: 0x09,
            value: configuration_value,
            index: 0,
            length,
        }),
    );
    mem.write_u32(EP0_RING + 8, 8);
    mem.write_u32(EP0_RING + 12, transfer_control(TRB_TYPE_SETUP_STAGE));
}

fn transfer_control(trb_type: u32) -> u32 {
    (trb_type << 10) | TRB_CYCLE
}
