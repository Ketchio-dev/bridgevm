//! Explicit global and PCI interrupt setup for the auto-clear regressions.

use super::*;

pub(super) fn program_vector0(xhci: &mut XhciController) {
    xhci.mmio_write(0x40, 4, xhci.mmio_read(0x40, 4) | 4);
    let base = u64::from(XHCI_MSIX_TABLE_OFFSET);
    xhci.mmio_write(base, 8, MSI_ADDRESS);
    xhci.mmio_write(base + 8, 4, u64::from(MSI_DATA));
}

pub(super) fn unmask_vector0(xhci: &mut XhciController) {
    xhci.mmio_write(u64::from(XHCI_MSIX_TABLE_OFFSET) + 12, 4, 0);
}

pub(super) fn mask_vector0(xhci: &mut XhciController) {
    xhci.mmio_write(u64::from(XHCI_MSIX_TABLE_OFFSET) + 12, 4, 1);
}

pub(super) fn expected_message() -> MsixMessage {
    MsixMessage {
        vector: 0,
        address: MSI_ADDRESS,
        data: MSI_DATA,
    }
}
