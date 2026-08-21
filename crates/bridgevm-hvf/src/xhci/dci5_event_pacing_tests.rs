use super::configure_endpoint_pointer_tests::{
    assert_short_packet_dci5_transfer_event, setup_configure_endpoint_with_pointer, DCI5,
    DCI5_BUFFER, DCI5_RING,
};
use super::configure_endpoint_tests::write_dci3_normal_trb;
use super::test_support::{TestRam, DOORBELL_BASE, EVENT_RING, TRB_SIZE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

#[test]
fn move_click_sequence_waits_for_guest_consumption_between_reports() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut xhci, &mut mem);
    for index in 1..3 {
        write_dci3_normal_trb(
            &mut mem,
            DCI5_RING + TRB_SIZE * index,
            DCI5_BUFFER + 0x20 * index,
            true,
        );
    }
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    let position = PointerPosition::new(1_024, 2_048).unwrap();
    xhci.queue_pointer_input_actions(&[
        PointerInputAction::Move(position),
        PointerInputAction::Click(position),
    ])
    .unwrap();

    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI5), &mut mem));
    assert_eq!(xhci.pointer_input_report_stats().emitted_button_reports, 0);
    assert!(xhci.mmio_write_with_mem(0x1038, 8, (EVENT_RING + TRB_SIZE * 2) | 8, &mut mem));
    assert_eq!(xhci.pointer_input_report_stats().emitted_release_reports, 0);
    assert!(xhci.mmio_write_with_mem(0x1038, 8, (EVENT_RING + TRB_SIZE * 3) | 8, &mut mem));

    for (index, report) in [[0, 0, 4, 0, 8, 0], [1, 0, 4, 0, 8, 0], [0, 0, 4, 0, 8, 0]]
        .iter()
        .enumerate()
    {
        assert_eq!(
            mem.read_bytes(DCI5_BUFFER + index as u64 * 0x20, 6)
                .unwrap(),
            *report
        );
        assert_short_packet_dci5_transfer_event(
            &mem,
            EVENT_RING + TRB_SIZE * (index as u64 + 1),
            DCI5_RING + TRB_SIZE * index as u64,
        );
    }
    let stats = xhci.pointer_input_report_stats();
    assert_eq!(
        (
            stats.emitted_move_reports,
            stats.emitted_button_reports,
            stats.emitted_release_reports
        ),
        (1, 1, 1)
    );
}
