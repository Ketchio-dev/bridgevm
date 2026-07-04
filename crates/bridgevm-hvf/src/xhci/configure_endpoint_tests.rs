use super::test_support::{
    assert_success_completion, command_control, setup_command_rings_with_parameter, TestRam,
    CMD_RING, DOORBELL_BASE, ENABLE_SLOT_ID, EVENT_RING, TRB_SIZE,
};
use super::*;

pub(super) const TRB_TYPE_CONFIGURE_ENDPOINT: u32 = 12;
pub(super) const TRB_TYPE_LINK: u32 = 6;
pub(super) const TRB_TYPE_NORMAL: u32 = 1;
pub(super) const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
pub(super) const INPUT_CONTEXT: u64 = 0x5000;
pub(super) const DCI3_RING: u64 = 0x6000;
pub(super) const DCI3_WRAP_RING: u64 = 0x6200;
pub(super) const DCI3_BUFFER: u64 = 0x6800;
pub(super) const DCI3_WRAP_BUFFER: u64 = 0x6820;
pub(super) const DCI3_INVALID_BUFFER: u64 = 0x8ffc;
pub(super) const OUTPUT_CONTEXT: u64 = 0x7000;
pub(super) const DCBAA: u64 = 0x4000;
pub(super) const DCI3: u32 = 3;
pub(super) const DCI3_INPUT_CONTEXT_OFFSET: u64 = 0x80;
pub(super) const DCI3_OUTPUT_CONTEXT_OFFSET: u64 = 0x60;
pub(super) const INPUT_CONTROL_DROP_CONTEXT_OFFSET: u64 = 0x00;
pub(super) const INPUT_CONTROL_ADD_CONTEXT_OFFSET: u64 = 0x04;
pub(super) const EP_CONTEXT_DWORD1_OFFSET: u64 = 0x04;
pub(super) const EP_TR_DEQUEUE_OFFSET: u64 = 0x08;
pub(super) const EP_CONTEXT_DWORD4_OFFSET: u64 = 0x10;
pub(super) const EP_CONTEXT_BYTES: usize = 32;
pub(super) const DCI3_ADD_CONTEXT_FLAG: u32 = 1 << DCI3;
pub(super) const DCI3_DWORD1: u32 = (3 << 1) | (3 << 3) | (8 << 16);
pub(super) const DCI3_DWORD4: u32 = 8;
pub(super) const TRB_CYCLE: u64 = 1;
pub(super) const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
pub(super) const COMPLETION_CODE_SUCCESS: u32 = 1;

#[test]
fn configure_endpoint_command_copies_dci3_context_and_posts_completion() {
    // Given: slot 1 Configure Endpoint names an input context adding HID interrupt IN DCI3.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);

    // When: the guest rings host-controller doorbell 0 for Configure Endpoint type 12.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // Then: DCI3 is copied into the output context and command completion advances CRCR.
    assert_eq!(
        mem.read_bytes(
            OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET,
            EP_CONTEXT_BYTES
        )
        .unwrap(),
        mem.read_bytes(INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET, EP_CONTEXT_BYTES)
            .unwrap()
    );
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert_success_completion(&mem, EVENT_RING, CMD_RING, ENABLE_SLOT_ID);
    assert_eq!(xhci.mmio_read(0x58, 8), CMD_RING + TRB_SIZE + TRB_CYCLE);
}

#[test]
fn slot1_dci3_doorbell_pends_when_no_report_is_queued() {
    // Given: Configure Endpoint has installed slot 1 HID interrupt IN DCI3.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the guest rings slot 1 doorbell target DCI3 with no report queued.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the interrupt IN endpoint NAKs — the Normal TD stays armed, the
    // buffer is untouched, no transfer event is posted, and the dequeue holds.
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
}

#[test]
fn slot1_dci3_doorbell_follows_link_trb_and_updates_output_dequeue() {
    // Given: DCI3 has one active Normal TRB, then a Link TRB with TC to a wrapped Normal TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_BUFFER, true);
    write_dci3_link_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_RING, true);
    write_dci3_normal_trb(&mut mem, DCI3_WRAP_RING, DCI3_WRAP_BUFFER, false);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the guest rings slot 1 DCI3 once before and once at the Link/TRB cycle frontier.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the wrapped Normal TRB with the toggled cycle is consumed and published.
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_WRAP_RING);
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_WRAP_RING + TRB_SIZE);
    assert!(!xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_WRAP_RING + TRB_SIZE
    );
}

#[test]
fn slot1_dci3_doorbell_ignores_normal_trb_with_inactive_cycle() {
    // Given: DCI3 is configured with DCS=1 but the next Normal TRB has cycle=0.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_BUFFER, false);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the guest rings slot 1 DCI3.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the inactive TRB is not consumed and no transfer event is posted.
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert!(xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_RING | TRB_CYCLE
    );
}

#[test]
fn slot1_dci3_link_trb_to_inactive_target_updates_output_dequeue_without_event() {
    // Given: DCI3 points at an active Link TRB with TC, but the wrapped Normal TRB is inactive.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_link_trb(&mut mem, DCI3_RING, DCI3_WRAP_RING, true);
    write_dci3_normal_trb(&mut mem, DCI3_WRAP_RING, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the guest rings slot 1 DCI3.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the Link TRB is consumed, the inactive Normal TRB is not, and no event is posted.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0xbb; 8]);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_WRAP_RING);
    assert!(!xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_WRAP_RING
    );
}

pub(super) fn setup_configure_endpoint_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    mem.write_u32(
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    mem.write_u32(
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    mem.write_u64(
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI3_RING | TRB_CYCLE,
    );
    mem.write_u32(
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_dci3_normal_trb(mem, DCI3_RING, DCI3_BUFFER, true);
    assert!(mem.write_bytes(DCI3_BUFFER, &[0xaa; 8]));
}

pub(super) fn write_dci3_normal_trb(mem: &mut TestRam, trb_gpa: u64, buffer_gpa: u64, cycle: bool) {
    mem.write_u64(trb_gpa, buffer_gpa);
    mem.write_u32(trb_gpa + 8, 8);
    mem.write_u32(trb_gpa + 12, (TRB_TYPE_NORMAL << 10) | u32::from(cycle));
}

fn write_dci3_link_trb(mem: &mut TestRam, trb_gpa: u64, target_gpa: u64, cycle: bool) {
    mem.write_u64(trb_gpa, target_gpa);
    mem.write_u32(
        trb_gpa + 12,
        (TRB_TYPE_LINK << 10) | TRB_LINK_TOGGLE_CYCLE | u32::from(cycle),
    );
}

pub(super) fn assert_success_dci3_transfer_event(mem: &TestRam, event_gpa: u64, trb_gpa: u64) {
    assert_eq!(mem.read_u64(event_gpa), trb_gpa);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, 0);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!((control >> 24) & 0xff, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}
