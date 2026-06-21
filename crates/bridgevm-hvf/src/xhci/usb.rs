pub(super) const DEVICE_DESCRIPTOR_LENGTH: u16 = 18;

const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE: u8 = 0x80;
const USB_DESCRIPTOR_TYPE_DEVICE: u8 = 1;

pub(super) const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];

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

pub(super) fn is_device_descriptor_request(packet: SetupPacket) -> bool {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    packet.bm_request_type == USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE
        && packet.request == USB_REQUEST_GET_DESCRIPTOR
        && descriptor_type == USB_DESCRIPTOR_TYPE_DEVICE
        && descriptor_index == 0
        && packet.index == 0
}
