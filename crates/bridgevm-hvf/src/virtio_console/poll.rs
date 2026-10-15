//! Host polling starts consuming queues only after driver initialization.

use super::*;
use crate::fwcfg::GuestMemoryMut;

pub(super) const DRIVER_OK: u32 = 4;

impl VirtioConsole {
    pub fn poll(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        if self.status & DRIVER_OK == 0 {
            return false;
        }
        let mut progressed = false;
        progressed |= self.process_control_tx_queue(mem);
        progressed |= self.flush_pending_control(mem);
        progressed |= self.deliver_agent_rx(mem);
        progressed |= self.process_agent_tx_queue(mem);
        progressed
    }
}
