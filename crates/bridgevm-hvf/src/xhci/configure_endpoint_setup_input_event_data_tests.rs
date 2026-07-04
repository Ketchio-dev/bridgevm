use super::configure_endpoint_tests::{
    setup_configure_endpoint_command, DCI3, DCI3_BUFFER, DCI3_OUTPUT_CONTEXT_OFFSET, DCI3_RING,
    DCI3_WRAP_BUFFER, DCI3_WRAP_RING, EP_TR_DEQUEUE_OFFSET, OUTPUT_CONTEXT, TRB_LINK_TOGGLE_CYCLE,
    TRB_TYPE_LINK, TRB_TYPE_NORMAL,
};
use super::test_support::{
    setup_secondary_event_ring, TestRam, DOORBELL_BASE, EVENT_RING, EVENT_RING1, TRB_SIZE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const WINDOWS_DCI3_COOKIE: u64 = 0xffff_920e_011c_1883;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
const INTERRUPTER_TARGET_1: u32 = 1 << 22;
const WINDOWS_DCI3_COOKIE2: u64 = 0xffff_998b_4921_d7e3;
const DCI3_BUFFER2: u64 = 0x6840;

#[test]
fn slot1_dci3_chained_event_data_td_posts_cookie_event_on_target_interrupter() {
    // Given: the observed Windows interrupt-IN TD is a chained Normal TRB,
    // followed by an Event Data TRB carrying the URB cookie on interrupter 1.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    setup_secondary_event_ring(&mut xhci, &mut mem);
    mem.write_u64(DCI3_RING, DCI3_BUFFER);
    mem.write_u32(DCI3_RING + 8, 8 | INTERRUPTER_TARGET_1);
    mem.write_u32(DCI3_RING + 12, (TRB_TYPE_NORMAL << 10) | TRB_CHAIN | 1);
    mem.write_u64(DCI3_RING + TRB_SIZE, WINDOWS_DCI3_COOKIE);
    mem.write_u32(DCI3_RING + TRB_SIZE + 8, INTERRUPTER_TARGET_1);
    mem.write_u32(
        DCI3_RING + TRB_SIZE + 12,
        (TRB_TYPE_EVENT_DATA << 10) | TRB_IOC | 1,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the guest rings the DCI3 doorbell.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the report posts one Event Data completion and advances past both TRBs.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_u64(EVENT_RING1), WINDOWS_DCI3_COOKIE);
    assert_eq!(mem.read_u32(EVENT_RING1 + 8) & 0x00ff_ffff, 8);
    assert_eq!(mem.read_u32(EVENT_RING1 + 8) >> 24, 1);
    let control = mem.read_u32(EVENT_RING1 + 12);
    assert_eq!((control >> 10) & 0x3f, 32);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!(control & TRANSFER_EVENT_ED, TRANSFER_EVENT_ED);
    assert_eq!(mem.read_u64(EVENT_RING1 + TRB_SIZE), 0);
    assert_eq!(mem.read_u64(EVENT_RING + (TRB_SIZE * 2)), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));
}

#[test]
fn slot1_dci3_repeated_chained_event_data_tds_post_cookie_events_and_advance() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    setup_secondary_event_ring(&mut xhci, &mut mem);
    write_dci3_chained_event_data_td(&mut mem, DCI3_RING, DCI3_BUFFER, WINDOWS_DCI3_COOKIE, true);
    write_dci3_chained_event_data_td(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 2),
        DCI3_BUFFER2,
        WINDOWS_DCI3_COOKIE2,
        true,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    assert_dci3_event_data_transfer_event(&mem, EVENT_RING1, WINDOWS_DCI3_COOKIE);
    assert_dci3_event_data_transfer_event(&mem, EVENT_RING1 + TRB_SIZE, WINDOWS_DCI3_COOKIE2);
    assert_eq!(mem.read_u64(EVENT_RING1 + (TRB_SIZE * 2)), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 4));
    assert_eq!(xhci.event_lifecycle_stats().transfer_event_posts, 2);
    assert_eq!(
        xhci.event_lifecycle_stats().last_event_parameter,
        WINDOWS_DCI3_COOKIE2
    );
}

#[test]
fn slot1_dci3_chained_event_data_td_follows_link_and_toggles_cycle() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    setup_secondary_event_ring(&mut xhci, &mut mem);
    write_dci3_chained_event_data_td(&mut mem, DCI3_RING, DCI3_BUFFER, WINDOWS_DCI3_COOKIE, true);
    write_dci3_link_trb(&mut mem, DCI3_RING + (TRB_SIZE * 2), DCI3_WRAP_RING, true);
    write_dci3_chained_event_data_td(
        &mut mem,
        DCI3_WRAP_RING,
        DCI3_WRAP_BUFFER,
        WINDOWS_DCI3_COOKIE2,
        false,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    assert_dci3_event_data_transfer_event(&mem, EVENT_RING1, WINDOWS_DCI3_COOKIE);
    assert_dci3_event_data_transfer_event(&mem, EVENT_RING1 + TRB_SIZE, WINDOWS_DCI3_COOKIE2);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_WRAP_RING + (TRB_SIZE * 2));
    assert!(!xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_WRAP_RING + (TRB_SIZE * 2)
    );
}

fn write_dci3_chained_event_data_td(
    mem: &mut TestRam,
    normal_gpa: u64,
    buffer_gpa: u64,
    event_data: u64,
    cycle: bool,
) {
    mem.write_u64(normal_gpa, buffer_gpa);
    mem.write_u32(normal_gpa + 8, 8 | INTERRUPTER_TARGET_1);
    mem.write_u32(
        normal_gpa + 12,
        (TRB_TYPE_NORMAL << 10) | TRB_CHAIN | u32::from(cycle),
    );
    mem.write_u64(normal_gpa + TRB_SIZE, event_data);
    mem.write_u32(normal_gpa + TRB_SIZE + 8, INTERRUPTER_TARGET_1);
    mem.write_u32(
        normal_gpa + TRB_SIZE + 12,
        (TRB_TYPE_EVENT_DATA << 10) | TRB_IOC | u32::from(cycle),
    );
}

fn write_dci3_link_trb(mem: &mut TestRam, trb_gpa: u64, target_gpa: u64, cycle: bool) {
    mem.write_u64(trb_gpa, target_gpa);
    mem.write_u32(
        trb_gpa + 12,
        (TRB_TYPE_LINK << 10) | TRB_LINK_TOGGLE_CYCLE | u32::from(cycle),
    );
}

fn assert_dci3_event_data_transfer_event(mem: &TestRam, event_gpa: u64, event_data: u64) {
    assert_eq!(mem.read_u64(event_gpa), event_data);
    assert_eq!(mem.read_u32(event_gpa + 8) & 0x00ff_ffff, 8);
    assert_eq!(mem.read_u32(event_gpa + 8) >> 24, 1);
    let control = mem.read_u32(event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, 32);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!(control & TRANSFER_EVENT_ED, TRANSFER_EVENT_ED);
    assert_eq!(control & 1, 1);
}
