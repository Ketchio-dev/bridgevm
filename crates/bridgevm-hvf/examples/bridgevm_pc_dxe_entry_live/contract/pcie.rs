use super::{expect, u32_at};
use bridgevm_hvf::pcie;

pub const FUNCTION_COUNT: usize = 8;
const RESULT_OFFSET: usize = 112;

#[derive(Debug, Eq, PartialEq)]
pub struct PcieProof {
    pub identities: [u32; FUNCTION_COUNT],
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
    Ok(PcieProof { identities })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut result = vec![0; RESULT_OFFSET + 4 + FUNCTION_COUNT * 4];
        result[RESULT_OFFSET..RESULT_OFFSET + 4]
            .copy_from_slice(&(FUNCTION_COUNT as u32).to_le_bytes());
        for (index, identity) in expected_identities().into_iter().enumerate() {
            let offset = RESULT_OFFSET + 4 + index * 4;
            result[offset..offset + 4].copy_from_slice(&identity.to_le_bytes());
        }
        result
    }

    #[test]
    fn accepts_all_versioned_endpoint_identities() {
        assert_eq!(
            validate(&fixture()).unwrap().identities,
            expected_identities()
        );
    }

    #[test]
    fn rejects_one_mismatched_firmware_identity() {
        let mut result = fixture();
        result[RESULT_OFFSET + 4 + 5 * 4] ^= 1;
        assert!(validate(&result).unwrap_err().contains("PCIe identity"));
    }
}
