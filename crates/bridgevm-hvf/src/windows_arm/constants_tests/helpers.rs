//! Shared fixtures.

use crate::windows_arm::*;

pub(super) fn test_uefi_fv_bytes(len: usize) -> Vec<u8> {
    assert!(len >= UEFI_FV_MIN_HEADER_BYTES);
    let header_length = 0x48_u16;
    let mut bytes = vec![0_u8; len];
    bytes[16..32].copy_from_slice(&[
        0x8c, 0x8c, 0xf9, 0x61, 0xd2, 0x4b, 0x2c, 0x4f, 0x8a, 0x89, 0x22, 0x4d, 0xaf, 0xdc, 0xf1,
        0x6f,
    ]);
    bytes[UEFI_FV_LENGTH_OFFSET..UEFI_FV_LENGTH_OFFSET + 8]
        .copy_from_slice(&(len as u64).to_le_bytes());
    bytes[UEFI_FV_SIGNATURE_OFFSET..UEFI_FV_SIGNATURE_OFFSET + 4]
        .copy_from_slice(UEFI_FV_SIGNATURE);
    bytes[0x2c..0x30].copy_from_slice(&0x0004_feff_u32.to_le_bytes());
    bytes[UEFI_FV_HEADER_LENGTH_OFFSET..UEFI_FV_HEADER_LENGTH_OFFSET + 2]
        .copy_from_slice(&header_length.to_le_bytes());
    bytes[0x34..0x36].copy_from_slice(&0_u16.to_le_bytes());
    bytes[0x36] = 0;
    bytes[0x37] = 2;
    bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x3c..0x40].copy_from_slice(&(len as u32).to_le_bytes());
    bytes[0x40..0x44].copy_from_slice(&0_u32.to_le_bytes());
    bytes[0x44..0x48].copy_from_slice(&0_u32.to_le_bytes());
    let checksum = 0_u16.wrapping_sub(uefi_checksum16(&bytes[..usize::from(header_length)]));
    bytes[0x32..0x34].copy_from_slice(&checksum.to_le_bytes());
    bytes
}
