pub(super) use super::usb_descriptors::CONFIGURATION_DESCRIPTOR;
use super::usb_descriptors::{
    DEVICE_DESCRIPTOR, HID_KEYBOARD_CLASS_DESCRIPTOR, HID_POINTER_CLASS_DESCRIPTOR,
    HID_POINTER_REPORT_DESCRIPTOR, HID_REPORT_DESCRIPTOR, STRING0_DESCRIPTOR,
};

const USB_REQUEST_CLEAR_FEATURE: u8 = 0x01;
const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQUEST_GET_CONFIGURATION: u8 = 0x08;
const USB_REQUEST_SET_CONFIGURATION: u8 = 0x09;
const USB_REQUEST_HID_SET_REPORT: u8 = 0x09;
const USB_REQUEST_HID_SET_IDLE: u8 = 0x0a;
const USB_REQUEST_HID_SET_PROTOCOL: u8 = 0x0b;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE: u8 = 0x80;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE: u8 = 0x81;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_DEVICE: u8 = 0x00;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_ENDPOINT: u8 = 0x02;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE: u8 = 0x21;
const USB_FEATURE_ENDPOINT_HALT: u16 = 0x0000;
const USB_ENDPOINT_ADDRESS_DCI3_INTERRUPT_IN: u16 = 0x0081;
const USB_ENDPOINT_ADDRESS_DCI5_INTERRUPT_IN: u16 = 0x0082;
const USB_DESCRIPTOR_TYPE_DEVICE: u8 = 1;
const USB_DESCRIPTOR_TYPE_CONFIGURATION: u8 = 2;
const USB_DESCRIPTOR_TYPE_STRING: u8 = 3;
const USB_DESCRIPTOR_TYPE_HID: u8 = 0x21;
const USB_DESCRIPTOR_TYPE_HID_REPORT: u8 = 0x22;

#[derive(Clone, Copy)]
pub(super) struct SetupPacket {
    pub(super) bm_request_type: u8,
    pub(super) request: u8,
    pub(super) value: u16,
    pub(super) index: u16,
    pub(super) length: u16,
}

pub(super) fn parse_setup_packet(parameter: u64) -> SetupPacket {
    let bytes = parameter.to_le_bytes();
    SetupPacket {
        bm_request_type: bytes[0],
        request: bytes[1],
        value: u16::from_le_bytes([bytes[2], bytes[3]]),
        index: u16::from_le_bytes([bytes[4], bytes[5]]),
        length: u16::from_le_bytes([bytes[6], bytes[7]]),
    }
}

#[derive(Clone, Copy)]
pub(super) enum ControlInData {
    Static(&'static [u8]),
    Byte(u8),
}

impl ControlInData {
    pub(super) const fn len(self) -> usize {
        match self {
            Self::Static(bytes) => bytes.len(),
            Self::Byte(_) => 1,
        }
    }
}

pub(super) fn data_in_for_setup_packet(
    packet: SetupPacket,
    current_configuration: u8,
) -> Option<ControlInData> {
    descriptor_for_setup_packet(packet)
        .map(ControlInData::Static)
        .or_else(|| get_configuration_for_setup_packet(packet, current_configuration))
}

pub(super) fn is_supported_setup_packet(packet: SetupPacket, current_configuration: u8) -> bool {
    set_configuration_value(packet).is_some()
        || is_hid_set_protocol_request(packet)
        || is_hid_set_idle_request(packet)
        || is_clear_endpoint_halt_request(packet)
        || is_hid_set_report_request(packet)
        || data_in_for_setup_packet(packet, current_configuration).is_some()
}

pub(super) fn is_hid_report_descriptor_get_request(packet: SetupPacket) -> bool {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    packet.bm_request_type == USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE
        && packet.request == USB_REQUEST_GET_DESCRIPTOR
        && descriptor_index == 0
        && descriptor_type == USB_DESCRIPTOR_TYPE_HID_REPORT
        && matches!(packet.index, 0 | 1)
}

pub(super) fn is_hid_class_descriptor_get_request(packet: SetupPacket) -> bool {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    packet.bm_request_type == USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE
        && packet.request == USB_REQUEST_GET_DESCRIPTOR
        && descriptor_index == 0
        && descriptor_type == USB_DESCRIPTOR_TYPE_HID
        && matches!(packet.index, 0 | 1)
}

fn descriptor_for_setup_packet(packet: SetupPacket) -> Option<&'static [u8]> {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    if packet.request != USB_REQUEST_GET_DESCRIPTOR || descriptor_index != 0 {
        return None;
    }
    match (packet.bm_request_type, descriptor_type) {
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_DEVICE)
            if packet.index == 0 =>
        {
            Some(&DEVICE_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_CONFIGURATION)
            if packet.index == 0 =>
        {
            Some(&CONFIGURATION_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_STRING)
            if packet.index == 0 =>
        {
            Some(&STRING0_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE, USB_DESCRIPTOR_TYPE_HID_REPORT)
            if packet.index == 0 =>
        {
            Some(&HID_REPORT_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE, USB_DESCRIPTOR_TYPE_HID_REPORT)
            if packet.index == 1 =>
        {
            Some(&HID_POINTER_REPORT_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE, USB_DESCRIPTOR_TYPE_HID)
            if packet.index == 0 =>
        {
            Some(&HID_KEYBOARD_CLASS_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE, USB_DESCRIPTOR_TYPE_HID)
            if packet.index == 1 =>
        {
            Some(&HID_POINTER_CLASS_DESCRIPTOR)
        }
        _ => None,
    }
}

fn get_configuration_for_setup_packet(
    packet: SetupPacket,
    current_configuration: u8,
) -> Option<ControlInData> {
    (packet.bm_request_type == USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE
        && packet.request == USB_REQUEST_GET_CONFIGURATION
        && packet.value == 0
        && packet.index == 0
        && packet.length == 1)
        .then_some(ControlInData::Byte(current_configuration))
}

pub(super) fn set_configuration_value(packet: SetupPacket) -> Option<u8> {
    if packet.bm_request_type != USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_DEVICE
        || packet.request != USB_REQUEST_SET_CONFIGURATION
        || packet.index != 0
        || packet.length != 0
    {
        return None;
    }
    match packet.value {
        0 => Some(0),
        1 => Some(1),
        _ => None,
    }
}

pub(super) fn is_hid_set_protocol_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_PROTOCOL
        && packet.value <= 1
        && matches!(packet.index, 0 | 1)
        && packet.length == 0
}

/// CLEAR_FEATURE(ENDPOINT_HALT) on the interrupt-IN endpoints (standard,
/// endpoint recipient, no data). Windows issues this to un-halt / reset the
/// keyboard or pointer interrupt endpoint; a STALL here is a device fault. The USB spec
/// also resets the endpoint's data toggle to DATA0 on this request.
pub(super) fn is_clear_endpoint_halt_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_ENDPOINT
        && packet.request == USB_REQUEST_CLEAR_FEATURE
        && packet.value == USB_FEATURE_ENDPOINT_HALT
        && matches!(
            packet.index,
            USB_ENDPOINT_ADDRESS_DCI3_INTERRUPT_IN | USB_ENDPOINT_ADDRESS_DCI5_INTERRUPT_IN
        )
        && packet.length == 0
}

/// HID SET_IDLE (class-interface OUT, no data). Windows sends this during
/// keyboard enumeration and treats a STALL as a device fault, resetting and
/// re-enumerating the keyboard. `value` carries duration<<8 | reportId, which
/// this stateless model accepts and acknowledges without storing.
pub(super) fn is_hid_set_idle_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_IDLE
        && matches!(packet.index, 0 | 1)
        && packet.length == 0
}

pub(super) fn is_hid_set_report_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_REPORT
        && packet.value == 0x0200
        && matches!(packet.index, 0 | 1)
        && packet.length == 1
}
