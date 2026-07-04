use super::usb::SetupPacket;
use super::XhciController;

pub const XHCI_HID_INTERFACE_COUNT: usize = 2;
pub const XHCI_HID_KEYBOARD_INTERFACE_INDEX: u16 = 0;
pub const XHCI_HID_POINTER_INTERFACE_INDEX: u16 = 1;
pub const XHCI_HID_PROTOCOL_BOOT: u8 = 0;
pub const XHCI_HID_PROTOCOL_REPORT: u8 = 1;
pub const XHCI_HID_BOOT_KEYBOARD_REPORT_BYTES: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XhciHidSemanticStats {
    pub total_setup_packets: u64,
    pub hid_report_descriptor_gets: u64,
    pub hid_report_descriptor_gets_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub last_hid_report_descriptor_length: u16,
    pub last_hid_report_descriptor_length_by_interface: [u16; XHCI_HID_INTERFACE_COUNT],
    pub hid_class_descriptor_gets: u64,
    pub hid_class_descriptor_gets_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub last_hid_class_descriptor_length: u16,
    pub last_hid_class_descriptor_length_by_interface: [u16; XHCI_HID_INTERFACE_COUNT],
    pub hid_set_protocol_boot: u64,
    pub hid_set_protocol_boot_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub hid_set_protocol_report: u64,
    pub hid_set_protocol_report_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub hid_set_idle: u64,
    pub hid_set_idle_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub hid_set_report: u64,
    pub hid_set_report_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub clear_endpoint_halt: u64,
    pub unsupported_setup_packets: u64,
    pub unsupported_setup_packets_by_interface: [u64; XHCI_HID_INTERFACE_COUNT],
    pub current_protocol: u8,
    pub current_protocol_by_interface: [u8; XHCI_HID_INTERFACE_COUNT],
    pub last_setup_bm_request_type: u8,
    pub last_setup_request: u8,
    pub last_setup_value: u16,
    pub last_setup_index: u16,
    pub last_setup_length: u16,
    pub last_unsupported_setup_bm_request_type: u8,
    pub last_unsupported_setup_request: u8,
    pub last_unsupported_setup_value: u16,
    pub last_unsupported_setup_index: u16,
    pub last_unsupported_setup_length: u16,
}

impl Default for XhciHidSemanticStats {
    fn default() -> Self {
        Self {
            total_setup_packets: 0,
            hid_report_descriptor_gets: 0,
            hid_report_descriptor_gets_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            last_hid_report_descriptor_length: 0,
            last_hid_report_descriptor_length_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            hid_class_descriptor_gets: 0,
            hid_class_descriptor_gets_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            last_hid_class_descriptor_length: 0,
            last_hid_class_descriptor_length_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            hid_set_protocol_boot: 0,
            hid_set_protocol_boot_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            hid_set_protocol_report: 0,
            hid_set_protocol_report_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            hid_set_idle: 0,
            hid_set_idle_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            hid_set_report: 0,
            hid_set_report_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            clear_endpoint_halt: 0,
            unsupported_setup_packets: 0,
            unsupported_setup_packets_by_interface: [0; XHCI_HID_INTERFACE_COUNT],
            current_protocol: XHCI_HID_PROTOCOL_REPORT,
            current_protocol_by_interface: [XHCI_HID_PROTOCOL_REPORT; XHCI_HID_INTERFACE_COUNT],
            last_setup_bm_request_type: 0,
            last_setup_request: 0,
            last_setup_value: 0,
            last_setup_index: 0,
            last_setup_length: 0,
            last_unsupported_setup_bm_request_type: 0,
            last_unsupported_setup_request: 0,
            last_unsupported_setup_value: 0,
            last_unsupported_setup_index: 0,
            last_unsupported_setup_length: 0,
        }
    }
}

impl XhciHidSemanticStats {
    pub const fn has_observations(self) -> bool {
        self.total_setup_packets != 0
            || self.hid_report_descriptor_gets != 0
            || self.hid_class_descriptor_gets != 0
            || self.hid_set_protocol_boot != 0
            || self.hid_set_protocol_report != 0
            || self.hid_set_idle != 0
            || self.hid_set_report != 0
            || self.clear_endpoint_halt != 0
            || self.unsupported_setup_packets != 0
    }

    pub const fn current_protocol_name(self) -> &'static str {
        Self::protocol_name(self.current_protocol)
    }

    pub const fn current_protocol_name_for_interface(self, interface_index: usize) -> &'static str {
        if interface_index < XHCI_HID_INTERFACE_COUNT {
            Self::protocol_name(self.current_protocol_by_interface[interface_index])
        } else {
            "unknown"
        }
    }

    pub const fn protocol_name(protocol: u8) -> &'static str {
        match protocol {
            XHCI_HID_PROTOCOL_BOOT => "boot",
            XHCI_HID_PROTOCOL_REPORT => "report",
            _ => "unknown",
        }
    }
}

impl XhciController {
    pub fn hid_semantic_stats(&self) -> XhciHidSemanticStats {
        self.hid_semantic_stats
    }

    pub(super) fn record_ep0_setup_packet(&mut self, packet: SetupPacket) {
        self.hid_semantic_stats.total_setup_packets = self
            .hid_semantic_stats
            .total_setup_packets
            .saturating_add(1);
        self.hid_semantic_stats.last_setup_bm_request_type = packet.bm_request_type;
        self.hid_semantic_stats.last_setup_request = packet.request;
        self.hid_semantic_stats.last_setup_value = packet.value;
        self.hid_semantic_stats.last_setup_index = packet.index;
        self.hid_semantic_stats.last_setup_length = packet.length;
    }

    pub(super) fn record_hid_report_descriptor_get(&mut self, interface_index: u16, length: u16) {
        self.hid_semantic_stats.hid_report_descriptor_gets = self
            .hid_semantic_stats
            .hid_report_descriptor_gets
            .saturating_add(1);
        self.hid_semantic_stats.last_hid_report_descriptor_length = length;
        if let Some(index) = hid_interface_stat_index(interface_index) {
            self.hid_semantic_stats
                .hid_report_descriptor_gets_by_interface[index] = self
                .hid_semantic_stats
                .hid_report_descriptor_gets_by_interface[index]
                .saturating_add(1);
            self.hid_semantic_stats
                .last_hid_report_descriptor_length_by_interface[index] = length;
        }
    }

    pub(super) fn record_hid_class_descriptor_get(&mut self, interface_index: u16, length: u16) {
        self.hid_semantic_stats.hid_class_descriptor_gets = self
            .hid_semantic_stats
            .hid_class_descriptor_gets
            .saturating_add(1);
        self.hid_semantic_stats.last_hid_class_descriptor_length = length;
        if let Some(index) = hid_interface_stat_index(interface_index) {
            self.hid_semantic_stats
                .hid_class_descriptor_gets_by_interface[index] = self
                .hid_semantic_stats
                .hid_class_descriptor_gets_by_interface[index]
                .saturating_add(1);
            self.hid_semantic_stats
                .last_hid_class_descriptor_length_by_interface[index] = length;
        }
    }

    pub(super) fn record_hid_set_protocol(&mut self, interface_index: u16, protocol: u16) {
        match protocol {
            value if value == u16::from(XHCI_HID_PROTOCOL_BOOT) => {
                self.hid_semantic_stats.hid_set_protocol_boot = self
                    .hid_semantic_stats
                    .hid_set_protocol_boot
                    .saturating_add(1);
                self.hid_semantic_stats.current_protocol = XHCI_HID_PROTOCOL_BOOT;
                if let Some(index) = hid_interface_stat_index(interface_index) {
                    self.hid_semantic_stats.hid_set_protocol_boot_by_interface[index] =
                        self.hid_semantic_stats.hid_set_protocol_boot_by_interface[index]
                            .saturating_add(1);
                    self.hid_semantic_stats.current_protocol_by_interface[index] =
                        XHCI_HID_PROTOCOL_BOOT;
                }
            }
            value if value == u16::from(XHCI_HID_PROTOCOL_REPORT) => {
                self.hid_semantic_stats.hid_set_protocol_report = self
                    .hid_semantic_stats
                    .hid_set_protocol_report
                    .saturating_add(1);
                self.hid_semantic_stats.current_protocol = XHCI_HID_PROTOCOL_REPORT;
                if let Some(index) = hid_interface_stat_index(interface_index) {
                    self.hid_semantic_stats.hid_set_protocol_report_by_interface[index] =
                        self.hid_semantic_stats.hid_set_protocol_report_by_interface[index]
                            .saturating_add(1);
                    self.hid_semantic_stats.current_protocol_by_interface[index] =
                        XHCI_HID_PROTOCOL_REPORT;
                }
            }
            _ => {}
        }
    }

    pub(super) fn record_hid_set_idle(&mut self, interface_index: u16) {
        self.hid_semantic_stats.hid_set_idle =
            self.hid_semantic_stats.hid_set_idle.saturating_add(1);
        if let Some(index) = hid_interface_stat_index(interface_index) {
            self.hid_semantic_stats.hid_set_idle_by_interface[index] =
                self.hid_semantic_stats.hid_set_idle_by_interface[index].saturating_add(1);
        }
    }

    pub(super) fn record_hid_set_report(&mut self, interface_index: u16) {
        self.hid_semantic_stats.hid_set_report =
            self.hid_semantic_stats.hid_set_report.saturating_add(1);
        if let Some(index) = hid_interface_stat_index(interface_index) {
            self.hid_semantic_stats.hid_set_report_by_interface[index] =
                self.hid_semantic_stats.hid_set_report_by_interface[index].saturating_add(1);
        }
    }

    pub(super) fn record_clear_endpoint_halt(&mut self) {
        self.hid_semantic_stats.clear_endpoint_halt = self
            .hid_semantic_stats
            .clear_endpoint_halt
            .saturating_add(1);
    }

    pub(super) fn record_unsupported_setup_packet(&mut self, packet: SetupPacket) {
        self.hid_semantic_stats.unsupported_setup_packets = self
            .hid_semantic_stats
            .unsupported_setup_packets
            .saturating_add(1);
        if setup_packet_has_interface_recipient(packet) {
            if let Some(index) = hid_interface_stat_index(packet.index) {
                self.hid_semantic_stats
                    .unsupported_setup_packets_by_interface[index] = self
                    .hid_semantic_stats
                    .unsupported_setup_packets_by_interface[index]
                    .saturating_add(1);
            }
        }
        self.hid_semantic_stats
            .last_unsupported_setup_bm_request_type = packet.bm_request_type;
        self.hid_semantic_stats.last_unsupported_setup_request = packet.request;
        self.hid_semantic_stats.last_unsupported_setup_value = packet.value;
        self.hid_semantic_stats.last_unsupported_setup_index = packet.index;
        self.hid_semantic_stats.last_unsupported_setup_length = packet.length;
    }
}

fn hid_interface_stat_index(interface_index: u16) -> Option<usize> {
    match interface_index {
        XHCI_HID_KEYBOARD_INTERFACE_INDEX => Some(0),
        XHCI_HID_POINTER_INTERFACE_INDEX => Some(1),
        _ => None,
    }
}

const fn setup_packet_has_interface_recipient(packet: SetupPacket) -> bool {
    packet.bm_request_type & 0x1f == 1
}
