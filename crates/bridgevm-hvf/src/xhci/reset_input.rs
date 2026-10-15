use super::XhciController;

impl XhciController {
    pub(super) fn reset_input_state(&mut self) {
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
    }
}
