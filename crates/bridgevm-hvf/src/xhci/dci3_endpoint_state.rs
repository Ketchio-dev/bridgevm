use super::XhciController;

impl XhciController {
    pub(super) fn invalidate_slot1_dci3_endpoint_state(&mut self) {
        self.slot1_dci3_dequeue = 0;
        self.slot1_dci3_ring_base = 0;
        self.slot1_dci3_dcs = false;
        self.slot1_dci3_two_entry_queue_rearm = false;
        self.slot1_dci3_last_dequeue = 0;
        self.slot1_dci3_last_dcs = false;
        self.slot1_dci3_last_ring_base = 0;
        self.slot1_dci3_last_ring_dcs = false;
    }

    pub(super) fn remember_slot1_dci3_endpoint_state(&mut self) {
        if self.slot1_dci3_dequeue != 0 && self.slot1_dci3_ring_base != 0 {
            self.slot1_dci3_last_dequeue = self.slot1_dci3_dequeue;
            self.slot1_dci3_last_dcs = self.slot1_dci3_dcs;
            self.slot1_dci3_last_ring_base = self.slot1_dci3_ring_base;
            self.slot1_dci3_last_ring_dcs = self.slot1_dci3_dcs;
        }
    }
}
