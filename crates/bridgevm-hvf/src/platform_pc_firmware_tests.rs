use super::*;
use crate::bridgevm_pc_boot_info::{BOOT_INFO_HEADER_SIZE, BOOT_INFO_MAGIC};

#[test]
fn firmware_bundle_binds_acpi_smbios_and_boot_info() {
    let bundle = BridgeVmPcPlatform::build_firmware_tables(4, 8 << 30).unwrap();
    assert_eq!(bundle.acpi.tables_base, bundle.boot_info.acpi_tables_gpa);
    assert_eq!(bundle.boot_info.bytes.len() as u64, board::BOOT_INFO.size);
    assert_eq!(&bundle.boot_info.bytes[..8], BOOT_INFO_MAGIC);
    assert_eq!(
        bundle.boot_info.bytes[..BOOT_INFO_HEADER_SIZE]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );
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
