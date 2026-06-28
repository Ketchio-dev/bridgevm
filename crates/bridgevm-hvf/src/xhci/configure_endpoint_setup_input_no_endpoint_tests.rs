use super::configure_endpoint_tests::*;
use super::test_support::{TestRam, DOORBELL_BASE};
use super::*;
use crate::fwcfg::GuestMemoryMut;

#[test]
fn queued_setup_input_without_dci3_endpoint_rejects_without_consuming_queue() {
    // Given: setup input is queued before any DCI3 endpoint state has been installed.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // When: the queued setup-input drain runs with no DCI3 dequeue/ring base.
    assert!(!xhci.process_queued_dci3_input(&mut mem));

    // Then: no report was emitted or consumed, so a later Configure Endpoint drains it.
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 0);
    assert_eq!(stats.emitted_release_reports, 0);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 0);
}
