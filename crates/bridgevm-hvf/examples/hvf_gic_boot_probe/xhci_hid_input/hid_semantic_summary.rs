use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{
    XhciHidSemanticStats, XHCI_HID_ABSOLUTE_POINTER_REPORT_BYTES,
    XHCI_HID_BOOT_KEYBOARD_REPORT_BYTES,
};

pub(crate) fn print_hid_semantic_summary(platform: &VirtPlatform) {
    let stats = platform.xhci_hid_semantic_stats();
    if !stats.has_observations() {
        return;
    }
    println!("xHCI HID semantic summary:");
    println!("  {}", format_hid_semantic_summary(stats));
}

fn format_hid_semantic_summary(stats: XhciHidSemanticStats) -> String {
    format!(
        "setup_packets={} unsupported_setup_packets={} hid_report_descriptor_gets={} last_hid_report_descriptor_length={} hid_set_protocol_boot={} hid_set_protocol_report={} hid_set_idle={} hid_set_report={} clear_endpoint_halt={} current_protocol={} current_protocol_value={} last_setup_bm_request_type={:#x} last_setup_request={:#x} last_setup_value={:#x} last_setup_index={:#x} last_setup_length={} keyboard_interrupt_in_report_bytes={} pointer_interrupt_in_report_bytes={} hid_report_descriptor_gets_interface0={} hid_report_descriptor_gets_interface1={} last_hid_report_descriptor_length_interface0={} last_hid_report_descriptor_length_interface1={} hid_class_descriptor_gets={} hid_class_descriptor_gets_interface0={} hid_class_descriptor_gets_interface1={} last_hid_class_descriptor_length={} last_hid_class_descriptor_length_interface0={} last_hid_class_descriptor_length_interface1={} hid_set_protocol_boot_interface0={} hid_set_protocol_boot_interface1={} hid_set_protocol_report_interface0={} hid_set_protocol_report_interface1={} hid_set_idle_interface0={} hid_set_idle_interface1={} hid_set_report_interface0={} hid_set_report_interface1={} unsupported_setup_packets_interface0={} unsupported_setup_packets_interface1={} current_protocol_interface0={} current_protocol_interface1={} current_protocol_value_interface0={} current_protocol_value_interface1={} last_unsupported_setup_bm_request_type={:#x} last_unsupported_setup_request={:#x} last_unsupported_setup_value={:#x} last_unsupported_setup_index={:#x} last_unsupported_setup_length={}",
        stats.total_setup_packets,
        stats.unsupported_setup_packets,
        stats.hid_report_descriptor_gets,
        stats.last_hid_report_descriptor_length,
        stats.hid_set_protocol_boot,
        stats.hid_set_protocol_report,
        stats.hid_set_idle,
        stats.hid_set_report,
        stats.clear_endpoint_halt,
        stats.current_protocol_name(),
        stats.current_protocol,
        stats.last_setup_bm_request_type,
        stats.last_setup_request,
        stats.last_setup_value,
        stats.last_setup_index,
        stats.last_setup_length,
        XHCI_HID_BOOT_KEYBOARD_REPORT_BYTES,
        XHCI_HID_ABSOLUTE_POINTER_REPORT_BYTES,
        stats.hid_report_descriptor_gets_by_interface[0],
        stats.hid_report_descriptor_gets_by_interface[1],
        stats.last_hid_report_descriptor_length_by_interface[0],
        stats.last_hid_report_descriptor_length_by_interface[1],
        stats.hid_class_descriptor_gets,
        stats.hid_class_descriptor_gets_by_interface[0],
        stats.hid_class_descriptor_gets_by_interface[1],
        stats.last_hid_class_descriptor_length,
        stats.last_hid_class_descriptor_length_by_interface[0],
        stats.last_hid_class_descriptor_length_by_interface[1],
        stats.hid_set_protocol_boot_by_interface[0],
        stats.hid_set_protocol_boot_by_interface[1],
        stats.hid_set_protocol_report_by_interface[0],
        stats.hid_set_protocol_report_by_interface[1],
        stats.hid_set_idle_by_interface[0],
        stats.hid_set_idle_by_interface[1],
        stats.hid_set_report_by_interface[0],
        stats.hid_set_report_by_interface[1],
        stats.unsupported_setup_packets_by_interface[0],
        stats.unsupported_setup_packets_by_interface[1],
        stats.current_protocol_name_for_interface(0),
        stats.current_protocol_name_for_interface(1),
        stats.current_protocol_by_interface[0],
        stats.current_protocol_by_interface[1],
        stats.last_unsupported_setup_bm_request_type,
        stats.last_unsupported_setup_request,
        stats.last_unsupported_setup_value,
        stats.last_unsupported_setup_index,
        stats.last_unsupported_setup_length
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::xhci::{XHCI_HID_PROTOCOL_BOOT, XHCI_HID_PROTOCOL_REPORT};
    use std::collections::BTreeMap;

    fn parse_summary_fields(summary: &str) -> BTreeMap<&str, &str> {
        summary
            .split_whitespace()
            .filter_map(|field| field.split_once('='))
            .collect()
    }

    #[test]
    fn hid_semantic_summary_is_parseable_for_protocol_and_descriptor_state() {
        // Given: HID stats collected from a Windows enumeration path.
        let mut stats = XhciHidSemanticStats::default();
        stats.total_setup_packets = 5;
        stats.hid_report_descriptor_gets = 1;
        stats.hid_report_descriptor_gets_by_interface[1] = 1;
        stats.last_hid_report_descriptor_length = 63;
        stats.last_hid_report_descriptor_length_by_interface[1] = 51;
        stats.hid_class_descriptor_gets = 1;
        stats.hid_class_descriptor_gets_by_interface[1] = 1;
        stats.last_hid_class_descriptor_length = 9;
        stats.last_hid_class_descriptor_length_by_interface[1] = 9;
        stats.hid_set_protocol_boot = 1;
        stats.hid_set_protocol_boot_by_interface[1] = 1;
        stats.hid_set_protocol_report = 0;
        stats.hid_set_idle = 1;
        stats.hid_set_idle_by_interface[1] = 1;
        stats.hid_set_report = 1;
        stats.hid_set_report_by_interface[1] = 1;
        stats.clear_endpoint_halt = 1;
        stats.unsupported_setup_packets = 1;
        stats.unsupported_setup_packets_by_interface[1] = 1;
        stats.current_protocol = XHCI_HID_PROTOCOL_BOOT;
        stats.current_protocol_by_interface[1] = XHCI_HID_PROTOCOL_BOOT;
        stats.last_setup_bm_request_type = 0x21;
        stats.last_setup_request = 0x09;
        stats.last_setup_value = 0x0200;
        stats.last_setup_index = 1;
        stats.last_setup_length = 1;
        stats.last_unsupported_setup_bm_request_type = 0x21;
        stats.last_unsupported_setup_request = 0x55;
        stats.last_unsupported_setup_value = 0x2100;
        stats.last_unsupported_setup_index = 1;
        stats.last_unsupported_setup_length = 9;

        // When: the live summary line is rendered.
        let summary = format_hid_semantic_summary(stats);
        let fields = parse_summary_fields(&summary);

        // Then: the line carries stable key=value fields for log classification.
        assert_eq!(fields.get("setup_packets").copied(), Some("5"));
        assert_eq!(fields.get("hid_report_descriptor_gets").copied(), Some("1"));
        assert_eq!(
            fields.get("hid_report_descriptor_gets_interface1").copied(),
            Some("1")
        );
        assert_eq!(
            fields.get("hid_class_descriptor_gets_interface1").copied(),
            Some("1")
        );
        assert_eq!(
            fields.get("hid_set_protocol_boot_interface1").copied(),
            Some("1")
        );
        assert_eq!(fields.get("hid_set_idle_interface1").copied(), Some("1"));
        assert_eq!(fields.get("hid_set_report_interface1").copied(), Some("1"));
        assert_eq!(
            fields.get("unsupported_setup_packets_interface1").copied(),
            Some("1")
        );
        assert_eq!(fields.get("current_protocol").copied(), Some("boot"));
        assert_eq!(
            fields.get("current_protocol_interface1").copied(),
            Some("boot")
        );
        assert_eq!(fields.get("last_setup_request").copied(), Some("0x9"));
        assert_eq!(
            fields.get("last_unsupported_setup_request").copied(),
            Some("0x55")
        );
        assert_eq!(
            fields.get("keyboard_interrupt_in_report_bytes").copied(),
            Some("8")
        );
        assert_eq!(
            fields.get("pointer_interrupt_in_report_bytes").copied(),
            Some("5")
        );
    }

    #[test]
    fn hid_semantic_summary_names_default_report_protocol() {
        // Given: no SET_PROTOCOL request changed the HID default.
        let stats = XhciHidSemanticStats::default();

        // When/Then: report protocol is rendered by name for live analysis.
        assert_eq!(stats.current_protocol, XHCI_HID_PROTOCOL_REPORT);
        assert_eq!(stats.current_protocol_name(), "report");
    }
}
