use crate::msix::MsixMessage;

use super::XhciController;

impl XhciController {
    pub fn raise_msix(
        &mut self,
        vector: u16,
        function_enabled: bool,
        function_masked: bool,
    ) -> Option<MsixMessage> {
        if !self.interrupt_pending_and_enabled() {
            return None;
        }
        self.msix.raise(vector, function_enabled, function_masked)
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
