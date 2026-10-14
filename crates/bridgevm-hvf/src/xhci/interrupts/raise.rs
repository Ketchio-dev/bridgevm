//! New xHCI requests obey the global gate before reaching PCI MSI-X state.

use super::*;

// xHCI 1.22 section 5.4.1: host-system interrupter generation enable.
const USB_CMD_INTE: u32 = 1 << 2;

impl XhciController {
    pub fn raise_pending_interrupter_msix_into(
        &mut self,
        function_enabled: bool,
        function_masked: bool,
        out: &mut Vec<MsixMessage>,
    ) {
        if self.usb_command & USB_CMD_INTE == 0 {
            return;
        }
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
}
