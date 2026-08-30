//! Versioned guest contract for the experimental BridgeVM Virtual ARM PC.
//!
//! This is an independent, ACPI-first board definition. It does not change the
//! current Windows board or make this board bootable; runtime and firmware must
//! opt in explicitly after their own gates are implemented.

use super::Region;

pub const BOARD_ID: &str = "com.ketchio.bridgevm.virtual-arm-pc";
pub const BOARD_ABI_VERSION: u32 = 1;
pub const SMBIOS_MANUFACTURER: &str = "Ketchio";
pub const SMBIOS_PRODUCT: &str = "BridgeVM Virtual ARM PC";

pub const FLASH_CODE: Region = Region::new(0x0000_0000, 0x0400_0000);
pub const FLASH_VARS: Region = Region::new(0x0400_0000, 0x0400_0000);
pub const GIC_DIST: Region = Region::new(0x2000_0000, 0x0001_0000);
pub const GIC_REDIST: Region = Region::new(0x2100_0000, 0x0200_0000);
pub const GIC_MSI_FRAME: Region = Region::new(0x2300_0000, 0x1_0000);
pub const UART: Region = Region::new(0x2400_0000, 0x1000);
pub const RTC: Region = Region::new(0x2401_0000, 0x1000);
pub const TPM_TIS: Region = Region::new(0x2500_0000, 0x5000);
pub const BOOT_INFO: Region = Region::new(0x2600_0000, 0x1_0000);
pub const PCIE_ECAM: Region = Region::new(0x4000_0000, 0x1000_0000);
pub const PCIE_MMIO_32: Region = Region::new(0x5000_0000, 0xB000_0000);
pub const RAM_BASE: u64 = 0x1_0000_0000;
pub const PCIE_MMIO_64: Region = Region::new(0x20_0000_0000, 0x20_0000_0000);
pub const PCIE_MMIO_64_NON_PREFETCH: Region = Region::new(PCIE_MMIO_64.base, PCIE_MMIO_64.size / 2);
pub const PCIE_MMIO_64_PREFETCH: Region =
    Region::new(PCIE_MMIO_64_NON_PREFETCH.end(), PCIE_MMIO_64.size / 2);

pub const GIC_MSI_INTID_BASE: u32 = 128;
pub const GIC_MSI_INTID_COUNT: u32 = 64;
pub const SPI_UART: u32 = 32;
pub const SPI_RTC: u32 = 33;
pub const SPI_TPM: u32 = 34;
pub const SPI_PCIE_INTA: u32 = 40;
pub const SPI_OFFSET: u32 = 32;
pub const PPI_PMU: u32 = 7;
pub const PPI_TIMER_HYP: u32 = 10;
pub const PPI_TIMER_VIRT: u32 = 11;
pub const PPI_TIMER_SECURE: u32 = 13;
pub const PPI_TIMER_NONSEC: u32 = 14;
pub const GICV3_REDIST_STRIDE: u64 = 0x2_0000;
pub const MAX_CPUS: u64 = 64;
const _: () = assert!(MAX_CPUS * GICV3_REDIST_STRIDE <= GIC_REDIST.size);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareDiscovery {
    UefiAcpiAndSmbios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceTransport {
    Pcie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDeviceContract {
    pub role: &'static str,
    pub bdf: &'static str,
    pub transport: DeviceTransport,
}

const fn pci_device(role: &'static str, bdf: &'static str) -> PciDeviceContract {
    PciDeviceContract {
        role,
        bdf,
        transport: DeviceTransport::Pcie,
    }
}

pub const PCI_DEVICES: [PciDeviceContract; 7] = [
    pci_device("system-storage", "00:01.0"),
    pci_device("usb-input", "00:02.0"),
    pci_device("installer-media", "00:03.0"),
    pci_device("network", "00:04.0"),
    pci_device("display", "00:05.0"),
    pci_device("guest-agent", "00:06.0"),
    pci_device("audio", "00:07.0"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardContract {
    pub id: &'static str,
    pub abi_version: u32,
    pub discovery: FirmwareDiscovery,
    pub firmware_handoff: &'static str,
    pub os_device_discovery: &'static str,
    pub ram_base: u64,
    pub max_cpus: u64,
}

pub const CONTRACT: BoardContract = BoardContract {
    id: BOARD_ID,
    abi_version: BOARD_ABI_VERSION,
    discovery: FirmwareDiscovery::UefiAcpiAndSmbios,
    firmware_handoff: "BridgeVM boot-info v1",
    os_device_discovery: "UEFI configuration tables and ACPI",
    ram_base: RAM_BASE,
    max_cpus: MAX_CPUS,
};

pub fn fixed_regions() -> [(&'static str, Region); 12] {
    [
        ("flash-code", FLASH_CODE),
        ("flash-vars", FLASH_VARS),
        ("gic-dist", GIC_DIST),
        ("gic-redist", GIC_REDIST),
        ("gic-msi-frame", GIC_MSI_FRAME),
        ("uart", UART),
        ("rtc", RTC),
        ("tpm-tis", TPM_TIS),
        ("boot-info", BOOT_INFO),
        ("pcie-ecam", PCIE_ECAM),
        ("pcie-mmio-32", PCIE_MMIO_32),
        ("pcie-mmio-64", PCIE_MMIO_64),
    ]
}

pub fn first_overlap() -> Option<(&'static str, &'static str)> {
    let regions = fixed_regions();
    for (index, (left_name, left)) in regions.iter().enumerate() {
        for (right_name, right) in regions.iter().skip(index + 1) {
            if left.overlaps(right) {
                return Some((*left_name, *right_name));
            }
        }
    }
    None
}

pub fn ram_region(ram_size: u64) -> Option<Region> {
    let ram = Region::new(RAM_BASE, ram_size);
    (ram_size > 0 && ram.end() <= PCIE_MMIO_64.base).then_some(ram)
}

pub const fn spi_to_intid(spi: u32) -> u32 {
    spi + SPI_OFFSET
}

pub const fn cpu_mpidr(cpu: u64) -> u64 {
    let aff0 = cpu % 16;
    let aff1 = cpu / 16;
    (aff1 << 8) | aff0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine;

    #[test]
    fn identity_and_version_are_immutable() {
        assert_eq!(CONTRACT.id, "com.ketchio.bridgevm.virtual-arm-pc");
        assert_eq!(CONTRACT.abi_version, 1);
        assert_eq!(SMBIOS_PRODUCT, "BridgeVM Virtual ARM PC");
    }

    #[test]
    fn fixed_regions_do_not_overlap() {
        assert_eq!(first_overlap(), None);
    }

    #[test]
    fn board_is_not_the_current_guest_address_contract() {
        assert_ne!(GIC_DIST, machine::GIC_DIST);
        assert_ne!(UART, machine::UART);
        assert_ne!(PCIE_ECAM, machine::PCIE_ECAM);
        assert_ne!(RAM_BASE, machine::RAM_BASE);
    }

    #[test]
    fn ram_stays_below_the_high_pcie_aperture() {
        assert_eq!(ram_region(0), None);
        assert_eq!(ram_region(8 << 30), Some(Region::new(RAM_BASE, 8 << 30)));
        assert_eq!(ram_region(PCIE_MMIO_64.base - RAM_BASE + 1), None);
        assert_eq!(PCIE_MMIO_64_NON_PREFETCH.base, PCIE_MMIO_64.base);
        assert_eq!(PCIE_MMIO_64_NON_PREFETCH.end(), PCIE_MMIO_64_PREFETCH.base);
        assert_eq!(PCIE_MMIO_64_PREFETCH.end(), PCIE_MMIO_64.end());
    }

    #[test]
    fn device_discovery_is_pcie_first_and_stable() {
        assert!(PCI_DEVICES
            .iter()
            .all(|device| device.transport == DeviceTransport::Pcie));
        assert_eq!(PCI_DEVICES[0].bdf, "00:01.0");
        assert_eq!(PCI_DEVICES[6].bdf, "00:07.0");
    }

    #[test]
    fn interrupt_and_cpu_numbering_are_deterministic() {
        assert_eq!(spi_to_intid(SPI_UART), 64);
        assert_eq!(PPI_TIMER_VIRT + 16, 27);
        assert_eq!(cpu_mpidr(0), 0);
        assert_eq!(cpu_mpidr(16), 0x100);
    }
}
