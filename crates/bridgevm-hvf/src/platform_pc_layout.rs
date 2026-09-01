use super::*;
use crate::machine::Region;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeVmPcMemoryLayout {
    pub flash_code: Region,
    pub flash_vars: Region,
    pub boot_info: Region,
    pub ram: Region,
}

impl BridgeVmPcPlatform {
    pub fn memory_layout(ram_size: u64) -> Option<BridgeVmPcMemoryLayout> {
        Some(BridgeVmPcMemoryLayout {
            flash_code: board::FLASH_CODE,
            flash_vars: board::FLASH_VARS,
            boot_info: board::BOOT_INFO,
            ram: board::ram_region(ram_size)?,
        })
    }
}
