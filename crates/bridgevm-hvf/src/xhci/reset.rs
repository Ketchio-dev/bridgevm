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
        self.slot1_ep0_dequeue = 0;
        self.slot1_dci3_dequeue = 0;
        self.slot1_dci3_dcs = false;
        self.reset_event_ring();
    }
}
