//! Minimal status/feature negotiation for the public BAR-path readiness fixture.
use super::super::*;
use super::helpers::*;

pub(super) fn initialize_status(dev: &mut VirtioPciNet, mem: &mut TestMem) {
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0, mem);
    assert_eq!(pci_read(dev, COMMON_DEVICE_STATUS, 1, mem), 0);
    mem.bytes.fill(0);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 1, mem);
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 3, mem);
    for (selector, accepted) in [(0, 0), (1, 1)] {
        pci_write(dev, COMMON_DEVICE_FEATURE_SELECT, 4, selector, mem);
        let offered = pci_read(dev, COMMON_DEVICE_FEATURE, 4, mem);
        assert_eq!(offered & accepted, accepted);
        pci_write(dev, COMMON_DRIVER_FEATURE_SELECT, 4, selector, mem);
        pci_write(dev, COMMON_DRIVER_FEATURE, 4, accepted, mem);
    }
    pci_write(dev, COMMON_DEVICE_STATUS, 1, 0x0b, mem);
    assert_eq!(pci_read(dev, COMMON_DEVICE_STATUS, 1, mem), 0x0b);
}
