use super::{expect, u32_at};
#[path = "pcie_identities.rs"]
mod identities;
#[path = "pcie_nvme.rs"]
mod nvme;
#[path = "pcie_nvme_block.rs"]
mod nvme_block;
pub use nvme::NvmeBarProof;
pub use nvme_block::NvmeBlockIoProof;
pub const FUNCTION_COUNT: usize = 8;
const RESULT_OFFSET: usize = 112;

#[derive(Debug, Eq, PartialEq)]
pub struct PcieProof {
    pub identities: [u32; FUNCTION_COUNT],
    pub root_bridge_count: u32,
    pub enumeration_complete: u32,
    pub driver_binding_count: u32,
    pub supported_status: u32,
    pub connect_status: u32,
    pub nvme: NvmeBarProof,
    pub nvme_block: NvmeBlockIoProof,
}

pub fn validate(result: &[u8]) -> Result<PcieProof, String> {
    expect(
        "PCIe function count",
        u32_at(result, RESULT_OFFSET, "PCIe function count")?,
        FUNCTION_COUNT as u32,
    )?;
    let expected = identities::expected();
    let mut identities = [0; FUNCTION_COUNT];
    for (index, identity) in identities.iter_mut().enumerate() {
        *identity = u32_at(result, RESULT_OFFSET + 4 + index * 4, "PCIe identity")?;
        expect("PCIe identity", *identity, expected[index])?;
    }
    let root_bridge_count = u32_at(result, RESULT_OFFSET + 36, "PCI root bridge count")?;
    expect("PCI root bridge count", root_bridge_count, 1)?;
    let enumeration_complete = u32_at(result, RESULT_OFFSET + 40, "PCI enumeration state")?;
    expect("PCI enumeration state", enumeration_complete, 1)?;
    let driver_binding_count = u32_at(result, RESULT_OFFSET + 44, "PCI driver binding count")?;
    expect("PCI driver binding count", driver_binding_count, 2)?;
    let supported_status = u32_at(result, RESULT_OFFSET + 48, "PCI bus supported status")?;
    expect("PCI bus supported status", supported_status, 0)?;
    let connect_status = u32_at(result, RESULT_OFFSET + 52, "PCI bus connect status")?;
    expect("PCI bus connect status", connect_status, 0)?;
    let nvme = nvme::validate(result)?;
    let nvme_block = nvme_block::validate(result)?;
    Ok(PcieProof {
        identities,
        root_bridge_count,
        enumeration_complete,
        driver_binding_count,
        supported_status,
        connect_status,
        nvme,
        nvme_block,
    })
}
#[cfg(test)]
#[path = "pcie_test_fixture.rs"]
mod test_fixture;
#[cfg(test)]
#[path = "pcie_tests.rs"]
mod tests;
