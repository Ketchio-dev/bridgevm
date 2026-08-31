use super::{expect, u32_at};
use bridgevm_hvf::pcie;

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
}

fn expected_identities() -> [u32; FUNCTION_COUNT] {
    [
        (u32::from(pcie::HOST_BRIDGE_DEVICE_ID) << 16) | u32::from(pcie::HOST_BRIDGE_VENDOR_ID),
        (u32::from(pcie::NVME_DEVICE_ID) << 16) | u32::from(pcie::NVME_VENDOR_ID),
        (u32::from(pcie::XHCI_DEVICE_ID) << 16) | u32::from(pcie::XHCI_VENDOR_ID),
        (u32::from(pcie::VIRTIO_BLK_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_BLK_VENDOR_ID),
        (u32::from(pcie::VIRTIO_NET_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_NET_VENDOR_ID),
        (u32::from(pcie::VIRTIO_GPU_DEVICE_ID) << 16) | u32::from(pcie::VIRTIO_GPU_VENDOR_ID),
        (u32::from(pcie::VIRTIO_CONSOLE_DEVICE_ID) << 16)
            | u32::from(pcie::VIRTIO_CONSOLE_VENDOR_ID),
        (u32::from(pcie::HDA_DEVICE_ID) << 16) | u32::from(pcie::HDA_VENDOR_ID),
    ]
}

pub fn validate(result: &[u8]) -> Result<PcieProof, String> {
    expect(
        "PCIe function count",
        u32_at(result, RESULT_OFFSET, "PCIe function count")?,
        FUNCTION_COUNT as u32,
    )?;
    let expected = expected_identities();
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
    expect("PCI driver binding count", driver_binding_count, 1)?;
    let supported_status = u32_at(result, RESULT_OFFSET + 48, "PCI bus supported status")?;
    expect("PCI bus supported status", supported_status, 0)?;
    let connect_status = u32_at(result, RESULT_OFFSET + 52, "PCI bus connect status")?;
    expect("PCI bus connect status", connect_status, 0)?;
    Ok(PcieProof {
        identities,
        root_bridge_count,
        enumeration_complete,
        driver_binding_count,
        supported_status,
        connect_status,
    })
}

#[cfg(test)]
#[path = "pcie_tests.rs"]
mod tests;
