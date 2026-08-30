use bridgevm_hvf::machine::bridgevm_pc as board;
use sha2::{Digest, Sha256};

pub const RESULT_OFFSET: usize = 0x1000;
pub const PASS_RESULT: u32 = 1;
pub const EXPECTED_FD_SHA256: &str =
    "af815a96240bb3cfd2ab19f6c853b70f609bdfca78f4a0885a08fb3ff9dbdf41";

pub fn result_gpa() -> Result<u64, String> {
    board::RAM_BASE
        .checked_add(RESULT_OFFSET as u64)
        .ok_or_else(|| "BridgeVM PC reset-vector result GPA overflow".to_string())
}

pub fn validate_firmware(bytes: &[u8]) -> Result<String, String> {
    let expected_len = usize::try_from(board::FLASH_CODE.size)
        .map_err(|_| "BridgeVM PC flash-code size exceeds host usize".to_string())?;
    if bytes.len() != expected_len {
        return Err(format!(
            "reset-vector FD has {} bytes; expected {expected_len}",
            bytes.len()
        ));
    }
    let hash = Sha256::digest(bytes);
    let mut digest = String::with_capacity(64);
    for byte in hash {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        digest.push(HEX[(byte >> 4) as usize] as char);
        digest.push(HEX[(byte & 0xf) as usize] as char);
    }
    if digest != EXPECTED_FD_SHA256 {
        return Err(format!(
            "reset-vector FD digest {digest} does not match {EXPECTED_FD_SHA256}"
        ));
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_and_flash_contract_use_independent_board_addresses() {
        assert_eq!(board::FLASH_CODE.base, 0);
        assert_eq!(board::FLASH_CODE.size, 0x0400_0000);
        assert_eq!(result_gpa().unwrap(), 0x1_0000_1000);
    }

    #[test]
    fn firmware_validation_rejects_non_contract_size_before_hashing() {
        let error = validate_firmware(&[0; 64]).unwrap_err();
        assert!(error.contains("expected 67108864"));
    }

    #[test]
    fn pinned_digest_has_sha256_shape() {
        assert_eq!(EXPECTED_FD_SHA256.len(), 64);
        assert!(EXPECTED_FD_SHA256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
    }
}
