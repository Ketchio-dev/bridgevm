use super::{bytes_at, expect, u64_at, FV_OFFSET, FV_SIZE};
use bridgevm_hvf::machine::bridgevm_pc as board;
use sha2::{Digest, Sha256};

const EXPECTED_FD_SHA256: &str = "1227e77889f26cb19c0e2fef2b446b727c39fa652b863c21474692dd65128873";
const DXE_CORE_GUID: [u8; 16] = [
    0x7f, 0xcb, 0xa2, 0xd6, 0x18, 0x6a, 0x2f, 0x4e, 0xb4, 0x3b, 0x99, 0x20, 0xa7, 0x33, 0x70, 0x0a,
];
const PLATFORM_TABLES_GUID: [u8; 16] = [
    0x6d, 0x37, 0xf1, 0xb6, 0x28, 0x3e, 0x1b, 0x42, 0xa6, 0x4c, 0x2b, 0x5e, 0x0d, 0x18, 0x53, 0x97,
];

fn sha256(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let mut digest = String::with_capacity(64);
    for byte in hash {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        digest.push(HEX[(byte >> 4) as usize] as char);
        digest.push(HEX[(byte & 0xf) as usize] as char);
    }
    digest
}

pub fn validate(bytes: &[u8]) -> Result<String, String> {
    let expected_len = board::FLASH_CODE.size as usize;
    expect("DXE-entry FD size", bytes.len(), expected_len)?;
    let digest = sha256(bytes);
    expect("DXE-entry FD digest", digest.as_str(), EXPECTED_FD_SHA256)?;
    let fv = bytes
        .get(FV_OFFSET..FV_OFFSET + FV_SIZE)
        .ok_or_else(|| "DXE firmware volume is outside flash".to_string())?;
    expect("FV length", u64_at(fv, 0x20, "FV length")?, FV_SIZE as u64)?;
    expect(
        "FV signature",
        bytes_at::<4>(fv, 0x28, "FV signature")?,
        *b"_FVH",
    )?;
    expect(
        "DXE Core file GUID",
        bytes_at::<16>(fv, 0x78, "DXE Core file GUID")?,
        DXE_CORE_GUID,
    )?;
    expect(
        "PlatformTablesDxe file GUID",
        fv.windows(16).any(|window| window == PLATFORM_TABLES_GUID),
        true,
    )?;
    Ok(digest)
}
