use super::test_support::{
    assert_success_completion, command_control, setup_command_rings_with_parameter, TestRam,
    CMD_RING, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;

const DCBAA: u64 = 0x4000;
const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
const PARTIAL_OUTPUT_CONTEXT: u64 = 0x5fc0;
const OUTPUT_CONTEXT: u64 = 0x7200;
const TRB_CYCLE: u64 = 1;
const SLOT_INPUT_CONTEXT_OFFSET: u64 = 0x20;
const SLOT_CONTEXT_DWORD0: u64 = 0x00;
const SLOT_CONTEXT_DWORD1: u64 = 0x04;
const SLOT_CONTEXT_DWORD2: u64 = 0x08;
const SLOT_CONTEXT_DWORD3: u64 = 0x0c;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;
const EP0_OUTPUT_CONTEXT_OFFSET: u64 = 0x20;
const EP_CONTEXT_DWORD0_OFFSET: u64 = 0x0;
const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x4;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
const SLOT_STATE_ADDRESSED: u32 = 3 << 27;
const EP_STATE_MASK: u32 = 0x7;
const EP_STATE_RUNNING: u32 = 1;
const NON_SLOT1_ID: u32 = 2;
const SLOT_DWORD0: u32 = 0x1122_3344;
const SLOT_DWORD1: u32 = 0x5566_7788;
const SLOT_DWORD2: u32 = 0x99aa_bbcc;
const EP0_DWORD1: u32 = (3 << 1) | (4 << 3) | (64 << 16);
const EP0_DWORD4: u32 = 8;

#[test]
fn address_device_command_populates_output_context_from_dcbaa_slot_entry() {
    // Given: Address Device names input and output contexts through CRCR/DCBAAP.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);

    // When: the guest rings host-controller doorbell 0 for Address Device.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: firmware-visible output context reports the addressed slot and EP0 ring.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3),
        SLOT_STATE_ADDRESSED | ENABLE_SLOT_ID
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        EP0_RING | TRB_CYCLE
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_copies_input_slot_context_dwords_before_addressing() {
    // Given: Address Device input carries slot context dwords Windows reads back after addressing.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    mem.write_u32(
        INPUT_CONTEXT + SLOT_INPUT_CONTEXT_OFFSET + SLOT_CONTEXT_DWORD0,
        SLOT_DWORD0,
    );
    mem.write_u32(
        INPUT_CONTEXT + SLOT_INPUT_CONTEXT_OFFSET + SLOT_CONTEXT_DWORD1,
        SLOT_DWORD1,
    );
    mem.write_u32(
        INPUT_CONTEXT + SLOT_INPUT_CONTEXT_OFFSET + SLOT_CONTEXT_DWORD2,
        SLOT_DWORD2,
    );

    // When: Address Device completes for slot 1.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: output slot context publishes input dword0-2 and addressed state in dword3.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD0),
        SLOT_DWORD0
    );
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD1),
        SLOT_DWORD1
    );
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD2),
        SLOT_DWORD2
    );
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3),
        SLOT_STATE_ADDRESSED | ENABLE_SLOT_ID
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_publishes_ep0_output_context_as_running() {
    // Given: Address Device captures slot 1 EP0 from an input context and has a DCBAA output slot.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);

    // When: Address Device completes for slot 1.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: the guest-visible EP0 output context is running on the captured transfer ring.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD0_OFFSET)
            & EP_STATE_MASK,
        EP_STATE_RUNNING
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        EP0_RING | TRB_CYCLE
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_copies_ep0_endpoint_context_dwords() {
    // Given: EP0 input context carries the endpoint parameters EDK2 reads back.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    mem.write_u32(
        INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        EP0_DWORD1,
    );
    mem.write_u32(
        INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        EP0_DWORD4,
    );
    assert_eq!(
        mem.read_u32(INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET),
        EP0_DWORD1
    );
    assert_eq!(
        mem.read_u32(INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET),
        EP0_DWORD4
    );

    // When: Address Device completes for slot 1.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: output EP0 exposes the same endpoint context dwords firmware consumes.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET),
        EP0_DWORD1
    );
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET),
        EP0_DWORD4
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_skips_zero_dcbaa_slot_entry_and_still_completes() {
    // Given: DCBAA[slot 1] is zero while another output-context-shaped region has data.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3, 0xa5a5_a5a5);
    mem.write_u64(
        OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        0xb5b5_b5b5_b5b5_b5b5,
    );

    // When: the guest rings host-controller doorbell 0 for Address Device.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: command completion still posts and no output context is touched.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3),
        0xa5a5_a5a5
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        0xb5b5_b5b5_b5b5_b5b5
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_skips_truncated_output_context_and_still_completes() {
    // Given: DCBAA[slot 1] points at a context truncated before EP0 dequeue storage.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x5fec);
    setup_address_device_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        PARTIAL_OUTPUT_CONTEXT,
    );

    // When: the guest rings host-controller doorbell 0 for Address Device.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: no partial output context is published and command completion still posts.
    assert_eq!(
        mem.read_u32(PARTIAL_OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3),
        0
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

#[test]
fn address_device_command_for_non_slot1_does_not_mutate_slot1_output_context() {
    // Given: Address Device names slot 2 while DCBAA[slot 1] points at a sentinel context.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_command(&mut xhci, &mut mem, NON_SLOT1_ID);
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    mem.write_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3, 0xc5c5_c5c5);
    mem.write_u64(
        OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        0xd5d5_d5d5_d5d5_d5d5,
    );

    // When: the guest rings host-controller doorbell 0 for Address Device.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: the slot-1-only contract leaves slot 1 output context unchanged.
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3),
        0xc5c5_c5c5
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        0xd5d5_d5d5_d5d5_d5d5
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, NON_SLOT1_ID);
}

fn setup_address_device_command(xhci: &mut XhciController, mem: &mut TestRam, slot_id: u32) {
    mem.write_u64(
        INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        EP0_RING | TRB_CYCLE,
    );
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, slot_id),
    );
}
