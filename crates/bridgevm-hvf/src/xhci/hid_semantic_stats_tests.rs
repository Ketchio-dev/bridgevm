use super::hid_semantic_stats_test_support::{
    prepare_addressed_control_transfer, write_control_transfer_at, ControlTransferShape,
    DATA_STAGE_BUFFER, EP0_RING, HID_CLASS_DESCRIPTOR_LENGTH, HID_POINTER_REPORT_DESCRIPTOR_LENGTH,
    HID_REPORT_DESCRIPTOR_LENGTH,
};
use super::test_support::{
    assert_success_transfer_event_for_trb, SetupPacketFields, TestRam, DOORBELL_BASE, EVENT_RING,
    TRB_SIZE,
};
use super::*;

#[test]
fn hid_semantic_stats_record_report_descriptor_get_when_windows_reads_hid_layout() {
    // Given: Windows reads the HID report descriptor during keyboard enumeration.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x81,
            request: 0x06,
            value: 0x2200,
            index: 0,
            length: HID_REPORT_DESCRIPTOR_LENGTH,
        },
        ControlTransferShape::DataIn {
            buffer: DATA_STAGE_BUFFER,
            length: HID_REPORT_DESCRIPTOR_LENGTH,
        },
    );

    // When: the guest rings EP0.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the low-volume HID summary records the descriptor read and setup packet.
    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 1);
    assert_eq!(stats.hid_report_descriptor_gets, 1);
    assert_eq!(stats.hid_report_descriptor_gets_by_interface[0], 1);
    assert_eq!(stats.hid_report_descriptor_gets_by_interface[1], 0);
    assert_eq!(
        stats.last_hid_report_descriptor_length,
        HID_REPORT_DESCRIPTOR_LENGTH
    );
    assert_eq!(
        stats.last_hid_report_descriptor_length_by_interface[0],
        HID_REPORT_DESCRIPTOR_LENGTH
    );
    assert_eq!(stats.current_protocol, XHCI_HID_PROTOCOL_REPORT);
    assert_eq!(stats.last_setup_request, 0x06);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
}

#[test]
fn hid_semantic_stats_record_interface_one_hid_descriptor_requests() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x81,
            request: 0x06,
            value: 0x2100,
            index: 1,
            length: HID_CLASS_DESCRIPTOR_LENGTH,
        },
        ControlTransferShape::DataIn {
            buffer: DATA_STAGE_BUFFER,
            length: HID_CLASS_DESCRIPTOR_LENGTH,
        },
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    write_control_transfer_at(
        &mut mem,
        xhci.slot1_ep0_dequeue,
        SetupPacketFields {
            bm_request_type: 0x81,
            request: 0x06,
            value: 0x2200,
            index: 1,
            length: HID_POINTER_REPORT_DESCRIPTOR_LENGTH,
        },
        ControlTransferShape::DataIn {
            buffer: DATA_STAGE_BUFFER,
            length: HID_POINTER_REPORT_DESCRIPTOR_LENGTH,
        },
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 2);
    assert_eq!(stats.hid_class_descriptor_gets, 1);
    assert_eq!(stats.hid_class_descriptor_gets_by_interface[0], 0);
    assert_eq!(stats.hid_class_descriptor_gets_by_interface[1], 1);
    assert_eq!(
        stats.last_hid_class_descriptor_length,
        HID_CLASS_DESCRIPTOR_LENGTH
    );
    assert_eq!(
        stats.last_hid_class_descriptor_length_by_interface[1],
        HID_CLASS_DESCRIPTOR_LENGTH
    );
    assert_eq!(stats.hid_report_descriptor_gets, 1);
    assert_eq!(stats.hid_report_descriptor_gets_by_interface[0], 0);
    assert_eq!(stats.hid_report_descriptor_gets_by_interface[1], 1);
    assert_eq!(
        stats.last_hid_report_descriptor_length_by_interface[1],
        HID_POINTER_REPORT_DESCRIPTOR_LENGTH
    );
    assert_eq!(stats.unsupported_setup_packets, 0);
}

#[test]
fn hid_semantic_stats_record_class_protocol_idle_and_set_report_requests() {
    // Given: Windows sends the HID class requests that decide keyboard semantics.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);

    // When: SET_PROTOCOL selects boot protocol.
    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0b,
            value: u16::from(XHCI_HID_PROTOCOL_BOOT),
            index: 0,
            length: 0,
        },
        ControlTransferShape::NoData,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // When: SET_IDLE completes as a no-data class request.
    write_control_transfer_at(
        &mut mem,
        xhci.slot1_ep0_dequeue,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0a,
            value: 0,
            index: 0,
            length: 0,
        },
        ControlTransferShape::NoData,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // When: SET_REPORT completes with a one-byte output report payload.
    write_control_transfer_at(
        &mut mem,
        xhci.slot1_ep0_dequeue,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length: 1,
        },
        ControlTransferShape::DataOut {
            buffer: DATA_STAGE_BUFFER,
            payload: 0,
        },
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    // Then: the summary distinguishes protocol, idle, and report requests.
    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 3);
    assert_eq!(stats.hid_set_protocol_boot, 1);
    assert_eq!(stats.hid_set_protocol_boot_by_interface[0], 1);
    assert_eq!(stats.hid_set_protocol_boot_by_interface[1], 0);
    assert_eq!(stats.hid_set_protocol_report, 0);
    assert_eq!(stats.hid_set_idle, 1);
    assert_eq!(stats.hid_set_idle_by_interface[0], 1);
    assert_eq!(stats.hid_set_report, 1);
    assert_eq!(stats.hid_set_report_by_interface[0], 1);
    assert_eq!(stats.current_protocol, XHCI_HID_PROTOCOL_BOOT);
    assert_eq!(
        stats.current_protocol_by_interface[0],
        XHCI_HID_PROTOCOL_BOOT
    );
    assert_eq!(stats.last_setup_request, 0x09);
}

#[test]
fn hid_semantic_stats_record_interface_one_class_protocol_idle_and_set_report_requests() {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);

    prepare_addressed_control_transfer(
        &mut xhci,
        &mut mem,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0b,
            value: u16::from(XHCI_HID_PROTOCOL_BOOT),
            index: 1,
            length: 0,
        },
        ControlTransferShape::NoData,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    write_control_transfer_at(
        &mut mem,
        xhci.slot1_ep0_dequeue,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x0a,
            value: 0,
            index: 1,
            length: 0,
        },
        ControlTransferShape::NoData,
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    write_control_transfer_at(
        &mut mem,
        xhci.slot1_ep0_dequeue,
        SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 1,
            length: 1,
        },
        ControlTransferShape::DataOut {
            buffer: DATA_STAGE_BUFFER,
            payload: 0,
        },
    );
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));

    let stats = xhci.hid_semantic_stats();
    assert_eq!(stats.total_setup_packets, 3);
    assert_eq!(stats.hid_set_protocol_boot, 1);
    assert_eq!(stats.hid_set_protocol_boot_by_interface[0], 0);
    assert_eq!(stats.hid_set_protocol_boot_by_interface[1], 1);
    assert_eq!(stats.hid_set_idle, 1);
    assert_eq!(stats.hid_set_idle_by_interface[0], 0);
    assert_eq!(stats.hid_set_idle_by_interface[1], 1);
    assert_eq!(stats.hid_set_report, 1);
    assert_eq!(stats.hid_set_report_by_interface[0], 0);
    assert_eq!(stats.hid_set_report_by_interface[1], 1);
    assert_eq!(
        stats.current_protocol_by_interface[1],
        XHCI_HID_PROTOCOL_BOOT
    );
}

#[test]
fn set_report_with_immediate_data_completes() {
    // Given: Windows sets the keyboard LEDs with the one-byte report inside the
    // data stage TRB (IDT), so its parameter is the data, not an address.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    // Bytes past the report's length are not guest RAM either.
    for (parameter, slot) in [(0x5a5a_0000_0000_0000u64, 1u64), (0xffff_ffff_ffff_ff02, 2)] {
        let packet = SetupPacketFields {
            bm_request_type: 0x21,
            request: 0x09,
            value: 0x0200,
            index: 0,
            length: 1,
        };
        let shape = ControlTransferShape::DataOutImmediate { parameter };
        if slot == 1 {
            prepare_addressed_control_transfer(&mut xhci, &mut mem, packet, shape);
        } else {
            write_control_transfer_at(&mut mem, xhci.slot1_ep0_dequeue, packet, shape);
        }
        // When: the guest rings EP0.
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, 1, &mut mem));
    }
    // Then: both requests complete (a pending one stalled shutdown by ~2 s).
    assert_eq!(xhci.hid_semantic_stats().hid_set_report, 2);
    assert_success_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, EP0_RING);
}
