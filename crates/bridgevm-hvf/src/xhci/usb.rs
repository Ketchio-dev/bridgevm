const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQUEST_GET_CONFIGURATION: u8 = 0x08;
const USB_REQUEST_SET_CONFIGURATION: u8 = 0x09;
const USB_REQUEST_HID_SET_REPORT: u8 = 0x09;
const USB_REQUEST_HID_SET_IDLE: u8 = 0x0a;
const USB_REQUEST_HID_SET_PROTOCOL: u8 = 0x0b;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE: u8 = 0x80;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE: u8 = 0x81;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_DEVICE: u8 = 0x00;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE: u8 = 0x21;
const USB_DESCRIPTOR_TYPE_DEVICE: u8 = 1;
const USB_DESCRIPTOR_TYPE_CONFIGURATION: u8 = 2;
const USB_DESCRIPTOR_TYPE_STRING: u8 = 3;
const USB_DESCRIPTOR_TYPE_HID_REPORT: u8 = 0x22;
const HID_REPORT_DESCRIPTOR_LENGTH: u8 = 63;
const HID_REPORT_DESCRIPTOR_LENGTH_USIZE: usize = 63;

pub(super) const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];

pub(super) const CONFIGURATION_DESCRIPTOR: [u8; 34] = [
    9,
    2,
    34,
    0,
    1,
    1,
    0,
    0x80,
    50,
    9,
    4,
    0,
    0,
    1,
    0x03,
    0x01,
    0x01,
    0,
    9,
    0x21,
    0x11,
    0x01,
    0,
    1,
    USB_DESCRIPTOR_TYPE_HID_REPORT,
    HID_REPORT_DESCRIPTOR_LENGTH,
    0,
    7,
    5,
    0x81,
    0x03,
    8,
    0,
    10,
];

pub(super) const HID_REPORT_DESCRIPTOR: [u8; HID_REPORT_DESCRIPTOR_LENGTH_USIZE] = [
    0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x03, 0x95, 0x05, 0x75, 0x01,
    0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x03, 0x95, 0x06,
    0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xc0,
];

const STRING0_DESCRIPTOR: [u8; 4] = [4, USB_DESCRIPTOR_TYPE_STRING, 0x09, 0x04];

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

fn descriptor_for_setup_packet(packet: SetupPacket) -> Option<&'static [u8]> {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    if packet.request != USB_REQUEST_GET_DESCRIPTOR || descriptor_index != 0 || packet.index != 0 {
        return None;
    }
    match (packet.bm_request_type, descriptor_type) {
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_DEVICE) => {
            Some(&DEVICE_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_CONFIGURATION) => {
            Some(&CONFIGURATION_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE, USB_DESCRIPTOR_TYPE_STRING) => {
            Some(&STRING0_DESCRIPTOR)
        }
        (USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_INTERFACE, USB_DESCRIPTOR_TYPE_HID_REPORT) => {
            Some(&HID_REPORT_DESCRIPTOR)
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
        && packet.index == 0
        && packet.length == 0
}

/// HID SET_IDLE (class-interface OUT, no data). Windows sends this during
/// keyboard enumeration and treats a STALL as a device fault, resetting and
/// re-enumerating the keyboard. `value` carries duration<<8 | reportId, which
/// this stateless model accepts and acknowledges without storing.
pub(super) fn is_hid_set_idle_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_IDLE
        && packet.index == 0
        && packet.length == 0
}

pub(super) fn is_hid_set_report_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_REPORT
        && packet.value == 0x0200
        && packet.index == 0
        && packet.length == 1
}
