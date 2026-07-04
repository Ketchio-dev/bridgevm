use super::hid_semantic_stats_test_support::{
    prepare_addressed_control_transfer, ControlTransferShape, DATA_STAGE_BUFFER,
    HID_CLASS_DESCRIPTOR_LENGTH,
};
use super::test_support::{SetupPacketFields, TestRam, DOORBELL_BASE};
use super::*;

#[test]
fn hid_semantic_stats_record_unsupported_setup_packets_before_stall() {
    // Given: Windows or a regression sends an unmodeled class request.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x55,
            value: 0x2100,
            index: 1,
            length: HID_CLASS_DESCRIPTOR_LENGTH,
        },
        ControlTransferShape::NoData,
    );

    // When: the request is rejected.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the live summary can classify a descriptor/class-request blocker.
    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 1);
    assert_eq!(stats.unsupported_setup_packets, 1);
    assert_eq!(stats.unsupported_setup_packets_by_interface[0], 0);
    assert_eq!(stats.unsupported_setup_packets_by_interface[1], 1);
    assert_eq!(stats.last_unsupported_setup_bm_request_type, 0x21);
    assert_eq!(stats.last_setup_request, 0x55);
    assert_eq!(stats.last_unsupported_setup_request, 0x55);
    assert_eq!(stats.last_unsupported_setup_value, 0x2100);
    assert_eq!(stats.last_unsupported_setup_index, 1);
    assert_eq!(
        stats.last_unsupported_setup_length,
        HID_CLASS_DESCRIPTOR_LENGTH
    );
}

#[test]
fn hid_semantic_stats_do_not_bucket_device_recipient_unsupported_by_interface() {
    // Given: Windows asks for an unsupported device-recipient string descriptor.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x80,
            request: 0x06,
            value: 0x03ee,
            index: 0,
            length: 18,
        },
        ControlTransferShape::DataIn {
            buffer: DATA_STAGE_BUFFER,
            length: 18,
        },
    );

    // When: the request is rejected before any descriptor payload is written.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: wIndex=0 is not misreported as interface-0 HID evidence.
    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 1);
    assert_eq!(stats.unsupported_setup_packets, 1);
    assert_eq!(stats.unsupported_setup_packets_by_interface[0], 0);
    assert_eq!(stats.unsupported_setup_packets_by_interface[1], 0);
    assert_eq!(stats.last_unsupported_setup_bm_request_type, 0x80);
    assert_eq!(stats.last_unsupported_setup_request, 0x06);
    assert_eq!(stats.last_unsupported_setup_value, 0x03ee);
    assert_eq!(stats.last_unsupported_setup_index, 0);
    assert_eq!(stats.last_unsupported_setup_length, 18);
}
