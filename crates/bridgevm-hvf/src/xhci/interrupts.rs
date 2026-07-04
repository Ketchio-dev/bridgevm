use crate::msix::MsixMessage;

use super::event::{IMAN_INTERRUPT_ENABLE, IMAN_INTERRUPT_PENDING};
use super::XhciController;

impl XhciController {
    /// xHCI routes interrupter `i` to MSI-X vector `i`; raise one message per
    /// interrupter with an enabled pending interrupt.
    ///
    /// Per xHCI 4.17.5, when MSI/MSI-X interrupts are enabled `IMAN.IP` is
    /// cleared automatically as the message is sent. We therefore clear IP on
    /// each interrupter whose message is actually delivered (`msix.raise`
    /// returned `Some`), so a single posted event yields exactly one message
    /// and an un-acknowledged interrupter does not re-fire on every flush. When
    /// delivery is deferred because the vector or function is masked,
    /// `msix.raise` returns `None`, the PBA bit records the pending message, and
    /// we keep IP set so the guest still observes the pending interrupt.
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
        let mut messages = Vec::new();
        for vector in pending {
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.clear_interrupter_pending(vector);
                messages.push(message);
            }
        }
        messages
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        if !self.interrupt_pending_and_enabled() {
            return Vec::new();
        }
        let messages = self.msix.drain_pending(function_enabled, function_masked);
        // A message deferred while masked also clears IP once it is finally
        // sent (xHCI 4.17.5), matching the auto-clear on the direct raise path.
        for message in &messages {
            self.clear_interrupter_pending(message.vector);
        }
        messages
    }

    fn clear_interrupter_pending(&mut self, vector: u16) {
        if let Some(interrupter) = self.interrupters.get_mut(usize::from(vector)) {
            interrupter.iman &= !IMAN_INTERRUPT_PENDING;
        }
    }
}
