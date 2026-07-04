use super::event::Interrupter;
use super::XhciController;

impl XhciController {
    pub(super) fn reset_programmed_state(&mut self) {
        self.crcr = 0;
        self.command_dequeue = 0;
        self.command_cycle = false;
        self.dcbaap = 0;
        self.config = 0;
        self.port_status_change_pending = false;
        self.slot1_ep0_dequeue = 0;
        self.slot1_ep0_dcs = false;
        self.invalidate_slot1_dci3_endpoint_state();
        self.invalidate_slot1_dci5_endpoint_state();
        self.boot_keyboard_report_queue.clear();
        self.pointer_input_report_queue.clear();
        self.setup_input_report_stats.controller_reset_generation = self
            .setup_input_report_stats
            .controller_reset_generation
            .saturating_add(1);
        self.pointer_input_report_stats.controller_reset_generation = self
            .pointer_input_report_stats
            .controller_reset_generation
            .saturating_add(1);
        self.usb_configuration = 0;
        self.interrupters = [Interrupter::new(); super::event::XHCI_INTERRUPTER_COUNT];
    }
}
