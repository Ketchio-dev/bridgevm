use super::XhciController;

impl XhciController {
    pub(super) fn reset_programmed_state(&mut self) {
        self.crcr = 0;
        self.command_dequeue = 0;
        self.command_cycle = false;
        self.dcbaap = 0;
        self.config = 0;
        self.iman0 = 0;
        self.imod0 = 0;
        self.erstsz0 = 0;
        self.erstba0 = 0;
        self.erdp0 = 0;
        self.post_hcrst_port_status_change_pending = false;
        self.slot1_ep0_dequeue = 0;
        self.slot1_ep0_dcs = false;
        self.invalidate_slot1_dci3_endpoint_state();
        self.boot_keyboard_report_queue.clear();
        self.usb_configuration = 0;
        self.reset_event_ring();
    }
}
