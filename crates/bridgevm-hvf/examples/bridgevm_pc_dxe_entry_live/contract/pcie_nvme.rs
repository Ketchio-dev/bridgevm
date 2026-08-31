use super::{expect, u32_at};
use crate::contract::u64_at;
use bridgevm_hvf::machine::bridgevm_pc as board;
use bridgevm_hvf::nvme::NVME_VERSION_1_4_0;
use bridgevm_hvf::pcie::{CMD_BUS_MASTER, CMD_MEMORY_SPACE, NVME_BAR0_SIZE};
use std::fmt;

const RESULT_OFFSET: usize = 168;
const EXPECTED_CAPABILITIES: u64 = 0x20_0201_03ff;

#[derive(Debug, Eq, PartialEq)]
pub struct NvmeBarProof {
    pub read_count: u32,
    pub resource_type: u32,
    pub base: u64,
    pub length: u64,
    pub capabilities: u64,
    pub version: u32,
    pub command: u32,
}

fn base_is_valid(base: u64, length: u64) -> bool {
    let end = match base.checked_add(length) {
        Some(end) => end,
        None => return false,
    };
    base % u64::from(NVME_BAR0_SIZE) == 0
        && ((base >= board::PCIE_MMIO_32.base && end <= board::PCIE_MMIO_32.end())
            || (base >= board::PCIE_MMIO_64_NON_PREFETCH.base
                && end <= board::PCIE_MMIO_64_NON_PREFETCH.end()))
}

pub fn validate(result: &[u8]) -> Result<NvmeBarProof, String> {
    let proof = NvmeBarProof {
        read_count: u32_at(result, RESULT_OFFSET, "NVMe BAR read count")?,
        resource_type: u32_at(result, RESULT_OFFSET + 4, "NVMe BAR resource type")?,
        base: u64_at(result, RESULT_OFFSET + 8, "NVMe BAR base")?,
        length: u64_at(result, RESULT_OFFSET + 16, "NVMe BAR length")?,
        capabilities: u64_at(result, RESULT_OFFSET + 24, "NVMe CAP")?,
        version: u32_at(result, RESULT_OFFSET + 32, "NVMe version")?,
        command: u32_at(result, RESULT_OFFSET + 36, "NVMe PCI command")?,
    };
    expect("NVMe BAR read count", proof.read_count, 2)?;
    expect("NVMe BAR resource type", proof.resource_type, 0)?;
    expect("NVMe BAR length", proof.length, u64::from(NVME_BAR0_SIZE))?;
    expect("NVMe CAP", proof.capabilities, EXPECTED_CAPABILITIES)?;
    expect("NVMe version", proof.version, NVME_VERSION_1_4_0)?;
    let command_mask = u32::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER);
    expect(
        "NVMe PCI command",
        proof.command & command_mask,
        command_mask,
    )?;
    if !base_is_valid(proof.base, proof.length) {
        return Err(format!(
            "NVMe BAR base {:#x} is outside the board apertures",
            proof.base
        ));
    }
    Ok(proof)
}

impl fmt::Display for NvmeBarProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "nvme_bar_reads={} nvme_bar0={:#x} nvme_bar0_len={} nvme_cap={:#x} nvme_version={:#x} nvme_command={:#x}",
            self.read_count, self.base, self.length, self.capabilities, self.version, self.command
        )
    }
}

#[cfg(test)]
pub(super) fn write_fixture(result: &mut [u8]) {
    result[RESULT_OFFSET..RESULT_OFFSET + 4].copy_from_slice(&2u32.to_le_bytes());
    result[RESULT_OFFSET + 8..RESULT_OFFSET + 16]
        .copy_from_slice(&board::PCIE_MMIO_32.base.to_le_bytes());
    result[RESULT_OFFSET + 16..RESULT_OFFSET + 24]
        .copy_from_slice(&u64::from(NVME_BAR0_SIZE).to_le_bytes());
    result[RESULT_OFFSET + 24..RESULT_OFFSET + 32]
        .copy_from_slice(&EXPECTED_CAPABILITIES.to_le_bytes());
    result[RESULT_OFFSET + 32..RESULT_OFFSET + 36]
        .copy_from_slice(&NVME_VERSION_1_4_0.to_le_bytes());
    let command = u32::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER);
    result[RESULT_OFFSET + 36..RESULT_OFFSET + 40].copy_from_slice(&command.to_le_bytes());
}

#[cfg(test)]
pub(super) fn fixture_proof() -> NvmeBarProof {
    NvmeBarProof {
        read_count: 2,
        resource_type: 0,
        base: board::PCIE_MMIO_32.base,
        length: u64::from(NVME_BAR0_SIZE),
        capabilities: EXPECTED_CAPABILITIES,
        version: NVME_VERSION_1_4_0,
        command: u32::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    }
}
