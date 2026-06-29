use super::test_support::{
    assert_success_completion, command_control, setup_command_rings_with_parameter, TestRam,
    CMD_RING, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;

const DCBAA: u64 = 0x4000;
const INPUT_CONTEXT: u64 = 0x5000;
const EP0_RING: u64 = 0x6000;
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
const ADDRESS_DEVICE_BSR: u32 = 1 << 9;
const SLOT_STATE_MASK: u32 = 0x1f << 27;
const SLOT_STATE_DEFAULT: u32 = 2 << 27;
const EP_STATE_MASK: u32 = 0x7;
const EP_STATE_RUNNING: u32 = 1;
const SLOT_DWORD0: u32 = 0x1122_3344;
const SLOT_DWORD1: u32 = 0x5566_7788;
const SLOT_DWORD2: u32 = 0x99aa_bbcc;
const EP0_DWORD1: u32 = (3 << 1) | (4 << 3) | (64 << 16);

#[test]
fn address_device_command_with_bsr_keeps_slot_default_and_ep0_not_running() {
    // Given: Address Device BSR names a slot-1 input context and output context.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_address_device_bsr_command(&mut xhci, &mut mem, ENABLE_SLOT_ID);
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
    mem.write_u32(
        INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        EP0_DWORD1,
    );

    // When: the guest rings host-controller doorbell 0 for Address Device with BSR set.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: BSR copies context data without publishing an addressed/running EP0 slot.
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
        mem.read_u32(OUTPUT_CONTEXT + SLOT_CONTEXT_DWORD3) & SLOT_STATE_MASK,
        SLOT_STATE_DEFAULT
    );
    assert_ne!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD0_OFFSET)
            & EP_STATE_MASK,
        EP_STATE_RUNNING
    );
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        EP0_RING | TRB_CYCLE
    );
    assert_eq!(
        mem.read_u32(OUTPUT_CONTEXT + EP0_OUTPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET),
        EP0_DWORD1
    );
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
}

fn setup_address_device_bsr_command(xhci: &mut XhciController, mem: &mut TestRam, slot_id: u32) {
    mem.write_u64(
        INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        EP0_RING | TRB_CYCLE,
    );
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, slot_id) | ADDRESS_DEVICE_BSR,
    );
}
