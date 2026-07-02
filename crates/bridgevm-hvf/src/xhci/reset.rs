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
        self.boot_keyboard_report_queue.clear();
        self.usb_configuration = 0;
        self.interrupters = [Interrupter::new(); super::event::XHCI_INTERRUPTER_COUNT];
    }
}
