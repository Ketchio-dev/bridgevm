use super::event::Interrupter;
use super::XhciController;

impl XhciController {
    pub(super) fn reset_programmed_state(&mut self) {
        self.dnctrl = 0;
        self.crcr = 0;
        self.command_dequeue = 0;
        self.command_cycle = false;
        self.dcbaap = 0;
        self.config = 0;
        self.port_status_change_pending = false;
        self.reset_input_state();
        self.usb_configuration = 0;
        self.interrupters = [Interrupter::new(); super::event::XHCI_INTERRUPTER_COUNT];
        self.pending_interrupter_bits = 0;
        self.pending_enabled_interrupter_bits = 0;
    }
}
