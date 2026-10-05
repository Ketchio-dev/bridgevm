use super::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

struct NoDma;
impl GuestMemoryMut for NoDma {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        panic!("ECAM must not DMA")
    }
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        panic!("ECAM must not DMA")
    }
}

#[test]
fn accepts_exactly_the_modeled_platform_identities() {
    let mut platform = BridgeVmPcPlatform::new();
    let mut modeled = Vec::new();
    for device in 0..8 {
        let outcome = platform.on_mmio(
            board::PCIE_ECAM.base + (device << 15),
            MmioOp::Read { size: 4 },
            &mut NoDma,
        );
        if let MmioOutcome::ReadValue(identity) = outcome {
            if identity != u64::from(u32::MAX) {
                modeled.push(identity as u32);
            }
        }
    }
    assert_eq!(modeled, [0x0008_1b36, 0x0010_1b36, 0x000d_1b36]);
    let mut result = test_fixture::fixture();
    result[RESULT_OFFSET..RESULT_OFFSET + 4].copy_from_slice(&3u32.to_le_bytes());
    result[RESULT_OFFSET + 4..RESULT_OFFSET + 36].fill(0);
    for (index, identity) in modeled.iter().enumerate() {
        let start = RESULT_OFFSET + 4 + index * 4;
        result[start..start + 4].copy_from_slice(&identity.to_le_bytes());
    }
    let proof = validate(&result).unwrap();
    assert_eq!(proof.identities.as_slice(), modeled.as_slice());
    assert_eq!(proof.nvme, nvme::fixture_proof());
    assert_eq!(proof.nvme_block, nvme_block::fixture_proof());
}

#[test]
fn rejects_missing_extra_and_reserved_identities() {
    for count in [0u32, 1, 2, 4, 8] {
        let mut result = test_fixture::fixture();
        result[RESULT_OFFSET..RESULT_OFFSET + 4].copy_from_slice(&count.to_le_bytes());
        assert!(validate(&result)
            .unwrap_err()
            .contains("PCIe function count"));
    }
    for slot in 0..8 {
        let mut result = test_fixture::fixture();
        result[RESULT_OFFSET + 4 + slot * 4] ^= 1;
        assert!(validate(&result).unwrap_err().contains("PCIe identity"));
    }
}

#[test]
fn retains_all_pci_and_nvme_positive_requirements() {
    for (offset, mask) in [
        (148, 1),
        (152, 1),
        (156, 1),
        (160, 1),
        (164, 1),
        (168, 1),
        (172, 1),
        (176, 1),
        (184, 1),
        (192, 1),
        (200, 1),
        (204, 2),
        (208, 1),
        (212, 1),
        (216, 1),
        (220, 1),
        (224, 1),
        (232, 1),
    ] {
        let mut result = test_fixture::fixture();
        result[offset] ^= mask;
        assert!(validate(&result).is_err(), "accepted bad field at {offset}");
    }
}
