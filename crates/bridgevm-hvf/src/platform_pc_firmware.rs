//! Firmware-table bundle for the experimental BridgeVM Virtual ARM PC v1.

use super::BridgeVmPcPlatform;
use crate::acpi::{build_bridgevm_pc_acpi, BridgeVmPcAcpiBlobs};
use crate::machine::bridgevm_pc as board;
use crate::smbios::{build_bridgevm_pc_smbios, SmbiosBlobs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeVmPcFirmwareTables {
    pub acpi: BridgeVmPcAcpiBlobs,
    pub smbios: SmbiosBlobs,
}

impl BridgeVmPcPlatform {
    pub fn build_firmware_tables(
        cpu_count: u64,
        ram_size: u64,
    ) -> Result<BridgeVmPcFirmwareTables, String> {
        if !(1..=board::MAX_CPUS).contains(&cpu_count) {
            return Err(format!(
                "vCPU count {cpu_count} is outside BridgeVM PC v1 range 1..={}",
                board::MAX_CPUS
            ));
        }
        Self::memory_layout(ram_size)
            .ok_or_else(|| "RAM does not fit the BridgeVM PC v1 address map".to_string())?;
        Ok(BridgeVmPcFirmwareTables {
            acpi: build_bridgevm_pc_acpi(cpu_count),
            smbios: build_bridgevm_pc_smbios(cpu_count, ram_size),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_bundle_binds_acpi_smbios_and_board_memory() {
        let bundle = BridgeVmPcPlatform::build_firmware_tables(4, 8 << 30).unwrap();
        assert_eq!(bundle.acpi.tables_base, board::BOOT_INFO.base);
        let identity = String::from_utf8_lossy(&bundle.smbios.tables);
        assert!(identity.contains(board::SMBIOS_MANUFACTURER));
        assert!(identity.contains(board::SMBIOS_PRODUCT));
        assert!(!identity.contains("qemu"));
        assert!(!identity.contains("QEMU"));
    }

    #[test]
    fn firmware_bundle_rejects_out_of_contract_cpu_and_ram() {
        assert!(BridgeVmPcPlatform::build_firmware_tables(0, 8 << 30).is_err());
        assert!(BridgeVmPcPlatform::build_firmware_tables(board::MAX_CPUS + 1, 8 << 30).is_err());
        assert!(BridgeVmPcPlatform::build_firmware_tables(1, 0).is_err());
        assert!(BridgeVmPcPlatform::build_firmware_tables(
            1,
            board::PCIE_MMIO_64.base - board::RAM_BASE + 1
        )
        .is_err());
    }
}
