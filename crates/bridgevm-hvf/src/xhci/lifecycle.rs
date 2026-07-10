use crate::msix::MsixTable;
use crate::pcie::XHCI_MSIX_VECTOR_COUNT;

use super::event::{Interrupter, XHCI_INTERRUPTER_COUNT};
use super::{
    pointer_input_report, ports::initial_ports, setup_input_report, XhciController,
    XhciHidSemanticStats, XhciPointerInputReportStats, XhciSetupInputReportStats,
};

impl Default for XhciController {
    fn default() -> Self {
        Self::new()
    }
}

impl XhciController {
    pub fn new() -> Self {
        Self {
            msix: MsixTable::new(XHCI_MSIX_VECTOR_COUNT),
            ports: initial_ports(),
            usb_command: 0,
            dnctrl: 0,
            crcr: 0,
            command_dequeue: 0,
            command_cycle: false,
            dcbaap: 0,
            config: 0,
            interrupters: [Interrupter::new(); XHCI_INTERRUPTER_COUNT],
            pending_interrupter_bits: 0,
            pending_enabled_interrupter_bits: 0,
            port_status_change_pending: false,
            slot1_ep0_dequeue: 0,
            slot1_ep0_dcs: false,
            slot1_dci3_dequeue: 0,
            slot1_dci3_ring_base: 0,
            slot1_dci3_dcs: false,
            slot1_dci3_two_entry_queue_rearm: false,
            slot1_dci3_last_dequeue: 0,
            slot1_dci3_last_dcs: false,
            slot1_dci3_last_ring_base: 0,
            slot1_dci3_last_ring_dcs: false,
            slot1_dci3_last_reusable: false,
            slot1_dci5_dequeue: 0,
            slot1_dci5_ring_base: 0,
            slot1_dci5_dcs: false,
            slot1_dci5_last_drain_blocked: None,
            boot_keyboard_report_queue: setup_input_report::BootKeyboardReportQueue::default(),
            setup_input_report_stats: XhciSetupInputReportStats::default(),
            pointer_input_report_queue: pointer_input_report::PointerInputReportQueue::default(),
            pointer_input_report_stats: XhciPointerInputReportStats::default(),
            event_lifecycle_stats: Default::default(),
            hid_semantic_stats: XhciHidSemanticStats::default(),
            usb_configuration: 0,
        }
    }
}
