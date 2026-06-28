use super::configure_endpoint_tests::*;
use super::test_support::{
    command_control, setup_command_rings_with_parameter, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE,
};
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

#[test]
fn configure_endpoint_installs_dci3_state_when_output_context_mirror_is_absent() {
    // Given: Windows provides a DCI3 Configure Endpoint input context, but the
    // DCBAA slot output context is not available to mirror endpoint fields.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command_without_output_context(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // When: the delayed setup-input drain runs after Configure Endpoint.
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert!(xhci.process_queued_dci3_input(&mut mem));

    // Then: the host-owned endpoint state still drains the queued key/release.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
}

fn setup_configure_endpoint_command_without_output_context(
    xhci: &mut XhciController,
    mem: &mut TestRam,
) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
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
    write_dci3_normal_trb(mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
}
