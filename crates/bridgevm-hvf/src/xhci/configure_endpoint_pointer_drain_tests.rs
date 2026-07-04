use super::configure_endpoint_pointer_tests::{
    assert_short_packet_dci5_transfer_event, setup_configure_endpoint_with_pointer, DCI5,
    DCI5_BUFFER, DCI5_RING,
};
use super::configure_endpoint_tests::{
    write_dci3_normal_trb, DCI3_BUFFER, EP_TR_DEQUEUE_OFFSET, OUTPUT_CONTEXT, TRB_CYCLE,
};
use super::test_support::{TestRam, DOORBELL_BASE, EVENT_RING, TRB_SIZE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const DCI5_OUTPUT_CONTEXT_OFFSET: u64 = 0xa0;

#[test]
fn queued_pointer_drain_runs_while_setup_input_queue_has_reports() {
    // Given: keyboard and pointer reports are both queued while both interrupt
    // endpoints have ready transfer TRBs.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    xhci.queue_setup_input_actions(&[SetupInputAction::Space])
        .unwrap();
    let position = PointerPosition::new(12_288, 20_480).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: an unrelated MMIO write kicks queued host-side input delivery.
    assert!(xhci.mmio_write_with_mem(0x14, 4, 0, &mut mem));

    // Then: DCI5 is not starved behind the still-nonempty DCI3 setup queue.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [0, 0, 0x30, 0, 0x50]
    );
    assert_short_packet_dci5_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI5_RING);
    let setup_stats = xhci.setup_input_report_stats();
    assert_eq!(setup_stats.emitted_key_reports, 1);
    assert_eq!(setup_stats.emitted_release_reports, 0);
    let pointer_stats = xhci.pointer_input_report_stats();
    assert_eq!(pointer_stats.emitted_move_reports, 1);
}

#[test]
fn slot1_dci5_doorbell_emits_absolute_pointer_move_report() {
    // Given: pointer DCI5 is configured with one interrupt-IN buffer and a queued move.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    let position = PointerPosition::new(16_384, 8_192).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: Windows polls the pointer interrupt endpoint.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));

    // Then: the report encodes buttons=0, little-endian absolute X/Y.
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [0, 0, 0x40, 0, 0x20]
    );
    assert_short_packet_dci5_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI5_RING);
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING + TRB_SIZE);
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.queued_actions, 1);
    assert_eq!(stats.queued_reports, 1);
    assert_eq!(stats.emitted_move_reports, 1);
}

#[test]
fn slot1_dci5_click_action_emits_button_down_then_release() {
    // Given: pointer DCI5 has two buffers and a queued click action.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI5_RING + TRB_SIZE, DCI5_BUFFER + 0x20, true);
    assert!(mem.write_bytes(DCI5_BUFFER + 0x20, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    let position = PointerPosition::new(1_024, 2_048).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Click(position)])
        .unwrap();

    // When: the guest polls DCI5 twice.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));

    // Then: the first report presses button 1 and the second releases it at the same position.
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [1, 0, 0x04, 0, 0x08]
    );
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER + 0x20, 5).unwrap(),
        [0, 0, 0x04, 0, 0x08]
    );
    assert_short_packet_dci5_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI5_RING);
    assert_short_packet_dci5_transfer_event(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        DCI5_RING + TRB_SIZE,
    );
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.emitted_button_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
}

#[test]
fn queued_dci5_pointer_drain_rearms_reusable_ring_base_cycle() {
    // Given: DCI5 was configured with DCS=1, but the reusable ring base now
    // contains a fresh Normal TRB using the opposite cycle state.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    write_dci3_normal_trb(&mut mem, DCI5_RING, DCI5_BUFFER, false);
    assert!(mem.write_bytes(DCI5_BUFFER, &[0xaa; 8]));
    let position = PointerPosition::new(4_096, 12_288).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: host-side pointer injection drains the queued DCI5 report.
    assert!(xhci.process_queued_dci5_pointer_input(&mut mem));

    // Then: queued-drain re-syncs to the ring base cycle and emits the report.
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [0, 0, 0x10, 0, 0x30]
    );
    assert_short_packet_dci5_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI5_RING);
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING + TRB_SIZE);
    assert!(!xhci.slot1_dci5_dcs);
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.emitted_move_reports, 1);
}

#[test]
fn queued_dci5_pointer_drain_reacquires_ready_output_context_after_state_loss() {
    // Given: DCI5 output context and transfer TRB are still live, but the
    // controller shadow state was lost during later configure/reset churn.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI5_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI5_RING | TRB_CYCLE
    );
    xhci.slot1_dci5_dequeue = 0;
    xhci.slot1_dci5_ring_base = 0;
    xhci.slot1_dci5_dcs = false;

    let position = PointerPosition::new(8_192, 24_576).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: host-side pointer injection drains without a fresh DCI5 doorbell.
    assert!(xhci.process_queued_dci5_pointer_input(&mut mem));

    // Then: the ready output-context dequeue is reacquired and the report emits.
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [0, 0, 0x20, 0, 0x60]
    );
    assert_short_packet_dci5_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI5_RING);
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_dci5_ring_base, DCI5_RING);
    assert!(xhci.slot1_dci5_dcs);
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.emitted_move_reports, 1);
}

#[test]
fn queued_dci5_pointer_drain_waits_for_empty_ring_base() {
    // Given: DCI5 is configured, but the guest has not posted a transfer TRB yet.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(mem.write_bytes(DCI5_RING, &[0; TRB_SIZE as usize]));
    let position = PointerPosition::new(16_384, 16_384).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: host-side pointer injection tries to drain before a TRB is available.
    assert!(!xhci.process_queued_dci5_pointer_input(&mut mem));

    // Then: the queued report is preserved for the first real DCI5 transfer TRB.
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING);
    assert!(xhci.slot1_dci5_dcs);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.queued_reports, 1);
    assert_eq!(stats.emitted_move_reports, 0);

    write_dci3_normal_trb(&mut mem, DCI5_RING, DCI5_BUFFER, true);
    assert!(mem.write_bytes(DCI5_BUFFER, &[0xaa; 8]));
    assert!(xhci.process_queued_dci5_pointer_input(&mut mem));
    assert_eq!(
        mem.read_bytes(DCI5_BUFFER, 5).unwrap(),
        [0, 0, 0x40, 0, 0x40]
    );
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.emitted_move_reports, 1);
}

#[test]
fn dci5_doorbell_ignores_inactive_cycle_at_ring_base() {
    // Given: a guest DCI5 doorbell points at a TRB whose cycle does not match
    // the configured DCS.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    write_dci3_normal_trb(&mut mem, DCI5_RING, DCI5_BUFFER, false);
    assert!(mem.write_bytes(DCI5_BUFFER, &[0xaa; 8]));
    let position = PointerPosition::new(4_096, 12_288).unwrap();
    xhci.queue_pointer_input_actions(&[PointerInputAction::Move(position)])
        .unwrap();

    // When: the guest rings DCI5 directly.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));

    // Then: after-doorbell processing still treats the TRB as not owned.
    assert_eq!(mem.read_bytes(DCI5_BUFFER, 5).unwrap(), [0xaa; 5]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_dci5_dequeue, DCI5_RING);
    assert!(xhci.slot1_dci5_dcs);
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(stats.emitted_move_reports, 0);
}
