//! Existing BAR register aliases, shared by read and compatibility-ACK writes.
use super::*;

pub(crate) fn pci_to_mmio_offset(offset: u64, is_write: bool) -> Option<u64> {
    if (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(offset - PCI_COMMON_CFG_OFFSET);
    }
    if (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_CONFIG + offset - PCI_DEVICE_CFG_OFFSET);
    }
    if (PCI_NOTIFY_CFG_OFFSET..PCI_NOTIFY_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_QUEUE_NOTIFY);
    }
    if offset == PCI_ISR_CFG_OFFSET {
        return Some(if is_write {
            REG_INTERRUPT_ACK
        } else {
            REG_INTERRUPT_STATUS
        });
    }
    None
}
