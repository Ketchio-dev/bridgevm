use super::configure_endpoint_tests::*;
use super::test_support::{TestRam, DOORBELL_BASE, TRB_SIZE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

#[test]
fn queued_setup_input_after_hcrst_reacquires_only_fresh_dci3_output_dequeue() {
    // Given: DCI3 was configured before a real HCRST, leaving a stale diagnostic snapshot behind.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));
    xhci.slot1_dci3_last_dequeue = DCI3_RING;
    xhci.slot1_dci3_last_dcs = true;
    xhci.slot1_dci3_last_ring_base = DCI3_RING;
    xhci.slot1_dci3_last_ring_dcs = true;
    assert!(mem.write_bytes(DCI3_BUFFER, &[0xaa; 8]));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // When: queued setup-input drain runs before any post-reset DCI3 output dequeue exists.
    let drained = xhci.process_queued_dci3_input(&mut mem);

    // Then: the stale last snapshot is not used as a post-HCRST endpoint.
    assert!(!drained);
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 0);
    assert_eq!(stats.emitted_release_reports, 0);

    // When: Windows later provides a fresh DCI3 context after reset.
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert!(xhci.slot1_dci3_dcs);
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_RING | TRB_CYCLE
    );
    assert_eq!(mem.read_u64(DCI3_RING), DCI3_BUFFER);
    assert_eq!(mem.read_u32(DCI3_RING + 8), 8);
    assert_eq!(
        mem.read_u32(DCI3_RING + 12),
        (TRB_TYPE_NORMAL << 10) | TRB_CYCLE as u32
    );
    assert_eq!(mem.read_u64(DCI3_RING + TRB_SIZE), DCI3_WRAP_BUFFER);
    assert_eq!(xhci.setup_input_report_stats().queued_reports, 2);
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert!(xhci.process_queued_dci3_input(&mut mem));

    // Then: delayed setup-input drains from the fresh post-reset DCI3 endpoint.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
}
