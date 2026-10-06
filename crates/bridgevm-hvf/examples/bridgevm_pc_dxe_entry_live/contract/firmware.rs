use super::{bytes_at, expect, u64_at, FV_OFFSET, FV_SIZE};
use bridgevm_hvf::machine::bridgevm_pc as board;
use sha2::{Digest, Sha256};
#[path = "firmware_guids.rs"]
mod guids;
#[path = "variable_firmware_guid.rs"]
mod variable_guid;
use guids::{DXE_CORE, PLATFORM_TABLES, RUNTIME_DXE};
use variable_guid::VARIABLE_RUNTIME_DXE;
#[path = "approved_firmware.rs"]
mod approved_firmware;
#[cfg(test)]
#[path = "firmware_pin_tests.rs"]
mod pin_tests;
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn validate(bytes: &[u8]) -> Result<String, String> {
    let expected_len = board::FLASH_CODE.size as usize;
    expect("DXE-entry FD size", bytes.len(), expected_len)?;
    let digest = sha256(bytes);
    expect(
        "DXE-entry FD digest",
        digest.as_str(),
        approved_firmware::expected()?,
    )?;
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
