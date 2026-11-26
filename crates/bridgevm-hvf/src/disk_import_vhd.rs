//! A fixed VHD stores contiguous virtual sectors followed by a 512-byte footer.
//! Microsoft Virtual Hard Disk Image Format Specification, “Hard Disk Footer Format”.

use super::{be32, be64, Refusal, Result, MAX_VIRTUAL_BYTES};
use std::fs::File;
use std::os::unix::fs::FileExt;

pub(super) fn fixed_size(file: &File, file_len: u64) -> Result<u64> {
    let mut footer = [0u8; 512];
    let offset = file_len
        .checked_sub(footer.len() as u64)
        .ok_or(Refusal::NotAnImage)?;
    file.read_exact_at(&mut footer, offset)?;
    if &footer[..8] != b"conectix" {
        return Err(Refusal::NotAnImage.into());
    }

    let checksum = be32(&footer, 64);
    footer[64..68].fill(0);
    let sum: u32 = footer.iter().map(|&byte| u32::from(byte)).sum();
    if checksum != !sum {
        return Err(Refusal::Damaged("VHD footer checksum").into());
    }
    if be32(&footer, 12) != 0x0001_0000 || be32(&footer, 8) & !3 != 0 {
        return Err(Refusal::Unsupported("VHD footer".into()).into());
    }
    match be32(&footer, 60) {
        2 => {}
        3 => return Err(Refusal::DynamicVhd.into()),
        4 => return Err(Refusal::Differencing.into()),
        _ => return Err(Refusal::Unsupported("VHD disk type".into()).into()),
    }
    let size = be64(&footer, 48);
    if size == 0 || size % 512 != 0 || size != offset {
        return Err(Refusal::Damaged("fixed VHD size").into());
    }
    if size > MAX_VIRTUAL_BYTES {
        return Err(Refusal::TooLarge.into());
    }
    Ok(size)
}
