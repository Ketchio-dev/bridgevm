use super::{bytes_at, expect, u64_at, FV_OFFSET, FV_SIZE};
use bridgevm_hvf::machine::bridgevm_pc as board;
use sha2::{Digest, Sha256};
#[path = "firmware_guids.rs"]
mod guids;
#[path = "variable_firmware_guid.rs"]
mod variable_guid;
use guids::{DXE_CORE, PLATFORM_TABLES, RUNTIME_DXE};
use variable_guid::VARIABLE_RUNTIME_DXE;
const EXPECTED_FD_SHA256: &str = "42e294e45119d08a5a8d6b4f28b5de9b79872be9282d700832460977bbd8282b";
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
        DXE_CORE,
    )?;
    for (label, guid) in [
        ("RuntimeDxe", RUNTIME_DXE),
        ("VariableRuntimeDxe", VARIABLE_RUNTIME_DXE),
        ("PlatformTablesDxe", PLATFORM_TABLES),
    ] {
        expect(label, fv.windows(16).any(|window| window == guid), true)?;
    }
    Ok(digest)
}
