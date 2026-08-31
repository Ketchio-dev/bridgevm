use super::{expect, u32_at};
use crate::contract::u64_at;
use std::fmt;

const RESULT_OFFSET: usize = 208;
const BLOCK_SIZE: u32 = 512;
const LAST_BLOCK: u64 = 2047;
const LBA0_MARKER: u64 = 0x4d56_4547_4449_5242;

#[derive(Debug, Eq, PartialEq)]
pub struct NvmeBlockIoProof {
    pub handle_count: u32,
    pub block_size: u32,
    pub media_present: u32,
    pub read_count: u32,
    pub last_block: u64,
    pub lba0_marker: u64,
}

pub fn validate(result: &[u8]) -> Result<NvmeBlockIoProof, String> {
    let proof = NvmeBlockIoProof {
        handle_count: u32_at(result, RESULT_OFFSET, "NVMe Block I/O count")?,
        block_size: u32_at(result, RESULT_OFFSET + 4, "NVMe block size")?,
        media_present: u32_at(result, RESULT_OFFSET + 8, "NVMe media-present state")?,
        read_count: u32_at(result, RESULT_OFFSET + 12, "NVMe block-read count")?,
        last_block: u64_at(result, RESULT_OFFSET + 16, "NVMe last block")?,
        lba0_marker: u64_at(result, RESULT_OFFSET + 24, "NVMe LBA0 marker")?,
    };
    expect("NVMe Block I/O count", proof.handle_count, 1)?;
    expect("NVMe block size", proof.block_size, BLOCK_SIZE)?;
    expect("NVMe media-present state", proof.media_present, 1)?;
    expect("NVMe block-read count", proof.read_count, 1)?;
    expect("NVMe last block", proof.last_block, LAST_BLOCK)?;
    expect("NVMe LBA0 marker", proof.lba0_marker, LBA0_MARKER)?;
    Ok(proof)
}

impl fmt::Display for NvmeBlockIoProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "nvme_block_io={} nvme_block_size={} nvme_media_present={} nvme_block_reads={} nvme_last_block={} nvme_lba0_marker={:#x}",
            self.handle_count, self.block_size, self.media_present, self.read_count,
            self.last_block, self.lba0_marker
        )
    }
}

#[cfg(test)]
pub(super) fn write_fixture(result: &mut [u8]) {
    for (offset, value) in [(0, 1), (4, BLOCK_SIZE), (8, 1), (12, 1)] {
        result[RESULT_OFFSET + offset..RESULT_OFFSET + offset + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
    result[RESULT_OFFSET + 16..RESULT_OFFSET + 24].copy_from_slice(&LAST_BLOCK.to_le_bytes());
    result[RESULT_OFFSET + 24..RESULT_OFFSET + 32].copy_from_slice(&LBA0_MARKER.to_le_bytes());
}

#[cfg(test)]
pub(super) fn fixture_proof() -> NvmeBlockIoProof {
    NvmeBlockIoProof {
        handle_count: 1,
        block_size: BLOCK_SIZE,
        media_present: 1,
        read_count: 1,
        last_block: LAST_BLOCK,
        lba0_marker: LBA0_MARKER,
    }
}
