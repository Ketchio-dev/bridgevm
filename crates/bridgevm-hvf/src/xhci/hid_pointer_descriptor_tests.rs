use super::usb::{
    data_in_for_setup_packet, is_clear_endpoint_halt_request, is_hid_set_idle_request,
    is_hid_set_protocol_request, is_hid_set_report_request, ControlInData, SetupPacket,
    CONFIGURATION_DESCRIPTOR,
};

const HID_REPORT_DESCRIPTOR_REQUEST: u16 = 0x2200;

#[test]
fn configuration_descriptor_advertises_pointer_interface_when_hid_surface_is_enabled() {
    // Given: the xHCI device exposes the existing boot keyboard plus an absolute pointer HID interface.
    let descriptor = CONFIGURATION_DESCRIPTOR;

    // Then: the descriptor tree contains two interfaces and the second interrupt-IN endpoint.
    assert_eq!(descriptor[2], descriptor.len() as u8);
    assert_eq!(descriptor[3], 0);
    assert_eq!(descriptor[4], 2);
    assert!(descriptor
        .windows(9)
        .any(|window| window == [9, 4, 1, 0, 1, 0x03, 0x00, 0x00, 0]));
    assert!(descriptor
        .windows(7)
        .any(|window| window == [7, 5, 0x82, 0x03, 8, 0, 10]));
}

#[test]
fn interface_one_gets_pointer_hid_report_descriptor() {
    // Given: Windows asks interface 1 for its HID report descriptor.
    let packet = SetupPacket {
        bm_request_type: 0x81,
        request: 0x06,
        value: HID_REPORT_DESCRIPTOR_REQUEST,
        index: 1,
        length: 255,
    };

    // When: EP0 maps the setup packet to data.
    let Some(ControlInData::Static(descriptor)) = data_in_for_setup_packet(packet, 1) else {
        panic!("interface 1 HID report descriptor was not returned");
    };

    // Then: it is an absolute mouse/pointer descriptor with 16-bit X/Y absolute axes.
    assert!(descriptor.windows(2).any(|window| window == [0x09, 0x02]));
    assert!(descriptor.windows(2).any(|window| window == [0x09, 0x30]));
    assert!(descriptor.windows(2).any(|window| window == [0x09, 0x31]));
    assert!(descriptor
        .windows(3)
        .any(|window| window == [0x26, 0xff, 0x7f]));
    assert!(descriptor.windows(2).any(|window| window == [0x75, 0x10]));
    assert!(descriptor.windows(2).any(|window| window == [0x09, 0x38]));
    assert!(descriptor
        .windows(4)
        .any(|window| window == [0x15, 0x81, 0x25, 0x7f]));
    assert!(descriptor.windows(2).any(|window| window == [0x81, 0x06]));
}

#[test]
fn pointer_interface_class_requests_are_supported_for_absolute_pointer() {
    // Given: Windows sends class requests to the pointer interface.
    let set_idle = SetupPacket {
        bm_request_type: 0x21,
        request: 0x0a,
        value: 0,
        index: 1,
        length: 0,
    };
    let set_report = SetupPacket {
        bm_request_type: 0x21,
        request: 0x09,
        value: 0x0200,
        index: 1,
        length: 1,
    };
    let set_protocol = SetupPacket {
        bm_request_type: 0x21,
        request: 0x0b,
        value: 0,
        index: 1,
        length: 0,
    };
    let clear_halt = SetupPacket {
        bm_request_type: 0x02,
        request: 0x01,
        value: 0,
        index: 0x0082,
        length: 0,
    };

    // Then: protocol/idle/report and endpoint reset are accepted for interface 1.
    assert!(is_hid_set_idle_request(set_idle));
    assert!(is_hid_set_report_request(set_report));
    assert!(is_hid_set_protocol_request(set_protocol));
    assert!(is_clear_endpoint_halt_request(clear_halt));
}
