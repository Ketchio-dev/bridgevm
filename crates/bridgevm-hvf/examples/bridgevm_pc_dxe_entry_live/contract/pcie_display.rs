use super::pcie::PcieProof;
use super::pcie_devices_display::PcieDevices;
use std::fmt;

impl fmt::Display for PcieProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "pcie_functions={} pci_root_bridges={} pci_enumeration_complete={} pci_driver_bindings={} pci_supported_status={} pci_connect_status={} {}",
            self.identities.len(), self.root_bridge_count, self.enumeration_complete,
            self.driver_binding_count, self.supported_status, self.connect_status, PcieDevices(self)
        )
    }
}
