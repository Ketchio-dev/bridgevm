const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQUEST_GET_CONFIGURATION: u8 = 0x08;
const USB_REQUEST_SET_CONFIGURATION: u8 = 0x09;
const USB_REQUEST_HID_SET_REPORT: u8 = 0x09;
const USB_REQUEST_HID_SET_PROTOCOL: u8 = 0x0b;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE: u8 = 0x80;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_DEVICE: u8 = 0x00;
const USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE: u8 = 0x21;
const USB_DESCRIPTOR_TYPE_DEVICE: u8 = 1;
const USB_DESCRIPTOR_TYPE_CONFIGURATION: u8 = 2;
const USB_DESCRIPTOR_TYPE_STRING: u8 = 3;

pub(super) const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];

pub(super) const CONFIGURATION_DESCRIPTOR: [u8; 34] = [
    9, 2, 34, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 1, 0x03, 0x01, 0x01, 0, 9, 0x21, 0x11, 0x01, 0, 1,
    0x22, 63, 0, 7, 5, 0x81, 0x03, 8, 0, 10,
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
    if packet.bm_request_type != USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE
        || packet.request != USB_REQUEST_GET_DESCRIPTOR
        || descriptor_index != 0
        || packet.index != 0
    {
        return None;
    }
    match descriptor_type {
        USB_DESCRIPTOR_TYPE_DEVICE => Some(&DEVICE_DESCRIPTOR),
        USB_DESCRIPTOR_TYPE_CONFIGURATION => Some(&CONFIGURATION_DESCRIPTOR),
        USB_DESCRIPTOR_TYPE_STRING => Some(&STRING0_DESCRIPTOR),
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

pub(super) fn is_set_configuration_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_STANDARD_DEVICE
        && packet.request == USB_REQUEST_SET_CONFIGURATION
        && packet.value == 1
        && packet.index == 0
        && packet.length == 0
}

pub(super) fn is_hid_set_protocol_request(packet: SetupPacket) -> bool {
    packet.bm_request_type == USB_REQUEST_TYPE_HOST_TO_DEVICE_CLASS_INTERFACE
        && packet.request == USB_REQUEST_HID_SET_PROTOCOL
        && packet.value <= 1
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
