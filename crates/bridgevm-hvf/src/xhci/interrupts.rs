use crate::msix::MsixMessage;

use super::event::{IMAN_INTERRUPT_ENABLE, IMAN_INTERRUPT_PENDING};
use super::XhciController;

impl XhciController {
    /// xHCI routes interrupter `i` to MSI-X vector `i`; raise one message per
    /// interrupter with an enabled pending interrupt.
    pub fn raise_pending_interrupter_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let enabled_pending = IMAN_INTERRUPT_PENDING | IMAN_INTERRUPT_ENABLE;
        let pending: Vec<u16> = self
            .interrupters
            .iter()
            .enumerate()
            .filter(|(_, interrupter)| interrupter.iman & enabled_pending == enabled_pending)
            .map(|(index, _)| index as u16)
            .collect();
        pending
            .into_iter()
            .filter_map(|vector| self.msix.raise(vector, function_enabled, function_masked))
            .collect()
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        if !self.interrupt_pending_and_enabled() {
            return Vec::new();
        }
        self.msix.drain_pending(function_enabled, function_masked)
    }
}
