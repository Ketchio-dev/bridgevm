use super::{pcie, u32_at};

pub fn check(dxe: &[u8], stage: u32) -> Result<(), String> {
    if stage & 0x8000_0000 == 0 {
        return Ok(());
    }
    let identities = (0..pcie::FUNCTION_COUNT)
        .map(|index| u32_at(dxe, 116 + index * 4, "failed PCIe identity"))
        .collect::<Result<Vec<_>, _>>()?;
    Err(format!(
        "DXE probe failed with EFI status {}: PCI root bridges={} enumeration_complete={} driver_bindings={} supported_status={} connect_status={} identities={identities:x?}",
        stage & 0x7fff_ffff,
        u32_at(dxe, 148, "failed PCI root bridge count")?,
        u32_at(dxe, 152, "failed PCI enumeration state")?,
        u32_at(dxe, 156, "failed PCI driver binding count")?,
        u32_at(dxe, 160, "failed PCI supported status")?,
        u32_at(dxe, 164, "failed PCI connect status")?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_firmware_status_before_normal_contract_validation() {
        let dxe = [0; 168];
        let error = check(&dxe, 0x8000_000e).unwrap_err();
        assert!(error.contains("EFI status 14"));
        assert!(error.contains("PCI root bridges=0"));
    }
}
