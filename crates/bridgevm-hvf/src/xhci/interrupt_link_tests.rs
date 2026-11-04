use super::configure_endpoint_pointer_tests::{
    setup_configure_endpoint_with_pointer, DCI5, DCI5_BUFFER, DCI5_RING,
};
use super::configure_endpoint_tests::{
    setup_configure_endpoint_command, DCI3, DCI3_BUFFER, DCI3_RING,
};
use super::test_support::{TestRam, DOORBELL_BASE, EVENT_RING, TRB_SIZE};
use super::*;

const TRB_TYPE_NORMAL: u32 = 1;
const TRB_TYPE_LINK: u32 = 6;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_CYCLE: u32 = 1;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
const URB_COOKIE: u64 = 0xffff_dc0a_c794_d7a3;
/// The report's Normal TRB sits in the slot right before the Link TRB.
const LAST_SLOT: u64 = 2;

/// Windows' interrupt-IN TD is a Normal TRB chained to an Event Data TRB that
/// carries the URB cookie. With the Normal TRB in the last slot before the
/// segment's Link TRB, the Event Data TRB is the Link target's first slot, in
/// the next cycle pass. Read at a fixed offset instead, the report completed
/// without its cookie and left an orphan Event Data TRB that blocked the
/// endpoint, so input froze.
fn write_report_td_crossing_link(mem: &mut TestRam, ring: u64, buffer: u64) {
    let normal = ring + (TRB_SIZE * LAST_SLOT);
    mem.write_u64(normal, buffer);
    mem.write_u32(normal + 8, 8);
    mem.write_u32(normal + 12, (TRB_TYPE_NORMAL << 10) | TRB_CHAIN | TRB_CYCLE);
    let link = normal + TRB_SIZE;
    mem.write_u64(link, ring);
    mem.write_u32(
        link + 12,
        (TRB_TYPE_LINK << 10) | TRB_CHAIN | TRB_LINK_TOGGLE_CYCLE | TRB_CYCLE,
    );
    // Past the Link TRB the producer cycle bit is 0.
    mem.write_u64(ring, URB_COOKIE);
    mem.write_u32(ring + 8, 0);
    mem.write_u32(ring + 12, (TRB_TYPE_EVENT_DATA << 10) | TRB_IOC);
}

fn assert_event_data_completion(mem: &TestRam, endpoint: u32) {
    let event = EVENT_RING + TRB_SIZE;
    assert_eq!(mem.read_u64(event), URB_COOKIE);
    let control = mem.read_u32(event + 12);
    assert_eq!(control & TRANSFER_EVENT_ED, TRANSFER_EVENT_ED);
    assert_eq!((control >> 16) & 0x1f, endpoint);
}

#[test]
fn pointer_report_follows_link_trb_between_normal_and_event_data() {
    // Given: the pointer TD's Normal TRB is the last slot before the Link TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    write_report_td_crossing_link(&mut mem, DCI5_RING, DCI5_BUFFER);
    xhci.slot1_dci5_dequeue = DCI5_RING + (TRB_SIZE * LAST_SLOT);
    let position = PointerPosition::new(4_096, 12_288).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: the queued move drains.
    assert!(xhci.process_queued_dci5_pointer_input(&mut mem));

    // Then: it completes with the URB cookie.
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 6).unwrap(),
        [0, 0, 0x10, 0, 0x30, 0]
    );
    assert_event_data_completion(&mem, DCI5);

    // And the pointer keeps moving: the next report, in the next pass right
    // after the Event Data TRB, drains too.
    let next = DCI5_RING + TRB_SIZE;
    mem.write_u64(next, DCI5_BUFFER + 0x40);
    mem.write_u32(next + 8, 8);
    mem.write_u32(next + 12, TRB_TYPE_NORMAL << 10);
    let position = PointerPosition::new(8_192, 8_192).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();
    assert!(xhci.process_queued_dci5_pointer_input(&mut mem));
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER + 0x40, 6).unwrap(),
        [0, 0, 0x20, 0, 0x20, 0]
    );
}

#[test]
fn keyboard_report_follows_link_trb_between_normal_and_event_data() {
    // Given: the keyboard TD's Normal TRB is the last slot before the Link TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    write_report_td_crossing_link(&mut mem, DCI3_RING, DCI3_BUFFER);
    xhci.slot1_dci3_dequeue = DCI3_RING + (TRB_SIZE * LAST_SLOT);
    xhci.queue_setup_input_actions(&[SetupInputAction::Space])
        .unwrap();

    // When: the queued key drains.
    assert!(xhci.process_queued_dci3_input(&mut mem));

    // Then: the key report completes with the URB cookie past the Link TRB.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_event_data_completion(&mem, DCI3);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + TRB_SIZE);
    assert!(!xhci.slot1_dci3_dcs);
}
