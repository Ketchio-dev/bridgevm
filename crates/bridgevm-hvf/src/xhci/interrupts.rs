use crate::msix::MsixMessage;

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
        let mut messages = Vec::new();
        self.raise_pending_interrupter_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn raise_pending_interrupter_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        let mut pending = self.pending_enabled_interrupter_bits();
        while pending != 0 {
            let vector = pending.trailing_zeros() as u16;
            if let Some(message) = self.msix.raise(vector, function_enabled, function_masked) {
                self.clear_interrupter_pending(usize::from(vector));
                out.push(message);
            }
            pending &= !(1u32 << vector);
        }
    }

    pub fn drain_pending_msix(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
    ) -> Vec<MsixMessage> {
        let mut messages = Vec::new();
        self.drain_pending_msix_into(function_enabled, function_masked, &mut messages);
        messages
    }

    pub fn drain_pending_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if !self.interrupt_pending_and_enabled() {
            return;
        }
        let start = out.len();
        self.msix
            .drain_pending_into(function_enabled, function_masked, out);
        // A message deferred while masked also clears IP once it is finally
        // sent (xHCI 4.17.5), matching the auto-clear on the direct raise path.
        for message in &out[start..] {
            self.clear_interrupter_pending(usize::from(message.vector));
        }
    }
}
