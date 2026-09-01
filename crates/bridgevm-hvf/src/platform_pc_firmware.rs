//! Firmware-table bundle for the experimental BridgeVM Virtual ARM PC v1.

use super::BridgeVmPcPlatform;
use crate::acpi::{build_bridgevm_pc_acpi, BridgeVmPcAcpiBlobs};
use crate::bridgevm_pc_boot_info::{assemble_bridgevm_pc_boot_info, BridgeVmPcBootInfoImage};
use crate::machine::bridgevm_pc as board;
use crate::smbios::{build_bridgevm_pc_smbios, SmbiosBlobs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeVmPcFirmwareTables {
    pub acpi: BridgeVmPcAcpiBlobs,
    pub smbios: SmbiosBlobs,
    pub boot_info: BridgeVmPcBootInfoImage,
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
        let acpi = build_bridgevm_pc_acpi(cpu_count);
        let mut smbios = build_bridgevm_pc_smbios(cpu_count, ram_size);
        let boot_info = assemble_bridgevm_pc_boot_info(cpu_count, ram_size, &acpi, &mut smbios)?;
        Ok(BridgeVmPcFirmwareTables {
            acpi,
            smbios,
            boot_info,
        })
    }
}

#[cfg(test)]
#[path = "platform_pc_firmware_tests.rs"]
mod tests;
