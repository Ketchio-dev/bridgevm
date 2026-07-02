use super::test_support::*;
use super::*;
use crate::xhci::event::USB_STS_EINT;

const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_DISABLE_SLOT: u32 = 10;
const TRB_TYPE_EVALUATE_CONTEXT: u32 = 13;
const TRB_TYPE_SET_TR_DEQUEUE_POINTER: u32 = 16;
const STOP_ENDPOINT_OBSERVED_CONTROL: u32 = 0x0101_3c01;
const RESET_DEVICE_OBSERVED_CONTROL: u32 = 0x0100_4601;
const ADDRESS_DEVICE_SLOT_ID: u32 = 7;
const DISABLE_SLOT_ID: u32 = 4;
const STOP_ENDPOINT_SLOT_ID: u32 = 1;
const SET_TR_DEQUEUE_POINTER_SLOT_ID: u32 = 1;
const ENDPOINT_ID_EP0: u32 = 1;
const SET_TR_DEQUEUE_POINTER_OBSERVED_CONTROL: u32 = (SET_TR_DEQUEUE_POINTER_SLOT_ID << 24)
    | (ENDPOINT_ID_EP0 << 16)
    | (TRB_TYPE_SET_TR_DEQUEUE_POINTER << 10)
    | 1;
const EP0_RECOVERY_RING: u64 = 0x2220;
const SLOT1_INPUT_CONTEXT: u64 = 0x3000;
const SLOT1_EP0_RING: u64 = 0x3400;
const LINK_TARGET: u64 = 0x1110;
const LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const SLOT1_OUTPUT_CONTEXT: u64 = 0x5000;

#[test]
fn reset_device_command_posts_completion_and_resets_slot_state() {
    // Given: the observed winload sequence sends Reset Device after reading
    // the device descriptor through a BSR-addressed slot.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(&mut xhci, &mut mem, RESET_DEVICE_OBSERVED_CONTROL);
    xhci.slot1_dci3_dequeue = SLOT1_EP0_RING;
    xhci.slot1_dci3_ring_base = SLOT1_EP0_RING;
    xhci.usb_configuration = 1;

    // When: the guest rings host-controller doorbell 0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: a success completion posts and the slot returns to the default
    // (unconfigured, non-DCI3) state awaiting a fresh Address Device.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, 1);
    assert_eq!(xhci.slot1_dci3_dequeue, 0);
    assert_eq!(xhci.slot1_dci3_ring_base, 0);
    assert_eq!(xhci.usb_configuration, 0);
}

#[test]
fn enable_slot_command_posts_success_completion_event() {
    // Given: firmware-style command/event rings containing one Enable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 1);
    assert_eq!(
        xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT),
        u64::from(USB_STS_EINT)
    );

    xhci.mmio_write(0x1020, 4, 1);
    assert_eq!(xhci.mmio_read(0x1020, 4) & 1, 0);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
}

#[test]
fn command_completion_advances_guest_visible_crcr_readback() {
    // Given: firmware-style command/event rings containing one Enable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: CRCR readback reports the next command TRB while preserving cycle.
    assert_eq!(xhci.mmio_read(0x58, 8), CMD_RING + TRB_SIZE + 1);
}

#[test]
fn host_controller_reset_clears_command_and_runtime_programming() {
    // Given: command, interrupter, and runtime registers contain stale guest state.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write(0x1024, 4, 0x1357);

    // When: the guest resets the host controller.
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    // Then: command/runtime programming is clear for a fresh initialization.
    assert_eq!(xhci.mmio_read(0x58, 8), 0);
    assert_eq!(xhci.mmio_read(0x70, 8), 0);
    assert_eq!(xhci.mmio_read(0x78, 4), 0);
    assert_eq!(xhci.mmio_read(0x1020, 4), 0);
    assert_eq!(xhci.mmio_read(0x1024, 4), 0);
    assert_eq!(xhci.mmio_read(0x1028, 4), 0);
    assert_eq!(xhci.mmio_read(0x1030, 8), 0);
    assert_eq!(xhci.mmio_read(0x1038, 8), 0);
    assert_eq!(xhci.mmio_read(0x44, 4) & u64::from(USB_STS_EINT), 0);
}

#[test]
fn address_device_command_posts_success_completion_event() {
    // Given: a command/event ring pair containing one Address Device TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ADDRESS_DEVICE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event for that slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ADDRESS_DEVICE_SLOT_ID);
}

#[test]
fn disable_slot_command_posts_success_completion_event() {
    // Given: a command/event ring pair containing one Disable Slot TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_DISABLE_SLOT, DISABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: event ring receives a successful Command Completion Event for that slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, DISABLE_SLOT_ID);
}

#[test]
fn disable_slot_for_slot1_clears_usb_configuration_state() {
    // Given: slot 1 has stale USB configuration state from a previous enumeration.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    xhci.usb_configuration = 1;
    setup_command_rings(
        &mut xhci,
        &mut mem,
        command_control(TRB_TYPE_DISABLE_SLOT, ENABLE_SLOT_ID),
    );

    // When: the guest disables slot 1.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: the slot-local USB state is unconfigured again.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.usb_configuration, 0);
}

#[test]
fn address_device_for_slot1_clears_usb_configuration_state() {
    // Given: slot 1 is being re-addressed after a previous configured lifecycle.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    xhci.usb_configuration = 1;
    mem.write_u64(SLOT1_INPUT_CONTEXT + 0x40 + 8, SLOT1_EP0_RING | 1);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        SLOT1_INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );

    // When: the guest addresses slot 1.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: configuration state returns to default while the new EP0 ring is captured.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.usb_configuration, 0);
    assert_eq!(xhci.slot1_ep0_dequeue, SLOT1_EP0_RING);
}

#[test]
fn stop_endpoint_command_posts_success_completion_and_advances_crcr_for_observed_control() {
    // Given: a command/event ring pair containing the observed Stop Endpoint TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    setup_command_rings(&mut xhci, &mut mem, STOP_ENDPOINT_OBSERVED_CONTROL);

    // When: the guest rings host-controller doorbell 0.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: completion is posted for slot 1 and CRCR advances past the command.
    assert_eq!(
        (mem.read_u64(EVENT_RING), xhci.mmio_read(0x58, 8)),
        (CMD_RING, CMD_RING + TRB_SIZE + 1)
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, STOP_ENDPOINT_SLOT_ID);
}

#[test]
fn set_tr_dequeue_pointer_command_updates_ep0_dequeue_and_posts_completion() {
    // Given: firmware recovery points slot 1 EP0 at a new transfer-ring dequeue pointer.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    mem.write_u64(0x4000 + u64::from(ENABLE_SLOT_ID) * 8, SLOT1_OUTPUT_CONTEXT);
    mem.write_u64(SLOT1_OUTPUT_CONTEXT + 0x20 + 0x8, 0x4441);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        EP0_RECOVERY_RING | 1,
        SET_TR_DEQUEUE_POINTER_OBSERVED_CONTROL,
    );

    // When: the guest rings host-controller doorbell 0 for Set TR Dequeue Pointer.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: completion is posted and the EP0 software dequeue tracks the command pointer.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, SET_TR_DEQUEUE_POINTER_SLOT_ID);
    assert_eq!(xhci.slot1_ep0_dequeue, EP0_RECOVERY_RING);
    assert_eq!(
        mem.read_u64(SLOT1_OUTPUT_CONTEXT + 0x20 + 0x8),
        EP0_RECOVERY_RING | 1
    );
    assert_eq!(xhci.mmio_read(0x58, 8), CMD_RING + TRB_SIZE + 1);
}

#[test]
fn evaluate_context_command_posts_success_completion_and_advances_crcr() {
    // Given: firmware evaluates slot 1 context while DCI3-looking input data is present.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x8000);
    mem.write_u64(0x4000 + u64::from(ENABLE_SLOT_ID) * 8, SLOT1_OUTPUT_CONTEXT);
    mem.write_u32(SLOT1_INPUT_CONTEXT + 0x04, 1 << 3);
    mem.write_u64(SLOT1_INPUT_CONTEXT + 0x80 + 0x8, 0x6001);
    setup_command_rings_with_parameter(
        &mut xhci,
        &mut mem,
        SLOT1_INPUT_CONTEXT,
        command_control(TRB_TYPE_EVALUATE_CONTEXT, ENABLE_SLOT_ID),
    );

    // When: the guest rings host-controller doorbell 0 for Evaluate Context.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: completion is posted, CRCR advances, and DCI3 state is not fabricated.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.mmio_read(0x58, 8), CMD_RING + TRB_SIZE + 1);
    assert_eq!((xhci.slot1_dci3_dequeue, xhci.slot1_dci3_dcs), (0, false));
    assert_eq!(mem.read_u64(SLOT1_OUTPUT_CONTEXT + 0x60), 0);
}

#[test]
fn command_doorbell_advances_internal_dequeue_to_address_device() {
    // Given: a command ring with Enable Slot followed by Address Device.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0 once per command.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: the second completion names the second command TRB.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(
        &mem,
        EVENT_RING + TRB_SIZE,
        CMD_RING + TRB_SIZE,
        ENABLE_SLOT_ID,
    );
}

#[test]
fn erdp_update_keeps_next_event_enqueue_slot() {
    // Given: the guest has a command ring with two commands and one event ring segment.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: firmware consumes the first event and updates ERDP before ringing again.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write(0x1038, 8, (EVENT_RING + TRB_SIZE) | 0x8);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: ERDP bookkeeping does not rewind the producer enqueue slot.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(
        &mem,
        EVENT_RING + TRB_SIZE,
        CMD_RING + TRB_SIZE,
        ENABLE_SLOT_ID,
    );
}

#[test]
fn command_doorbell_follows_link_trb_to_address_device() {
    // Given: Enable Slot is followed by a Link TRB to an Address Device command.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5000);
    mem.write_u32(
        CMD_RING + 12,
        command_control(TRB_TYPE_ENABLE_SLOT, ENABLE_SLOT_ID),
    );
    mem.write_u64(CMD_RING + TRB_SIZE, LINK_TARGET);
    mem.write_u32(
        CMD_RING + TRB_SIZE + 12,
        command_control(TRB_TYPE_LINK, 0) | LINK_TOGGLE_CYCLE,
    );
    mem.write_u32(
        LINK_TARGET + 12,
        command_control_with_cycle(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID, false),
    );
    setup_event_ring(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0 once per command.
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);
    xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem);

    // Then: the second completion names the linked Address Device command.
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_success_completion(&mem, EVENT_RING + TRB_SIZE, LINK_TARGET, ENABLE_SLOT_ID);
}
