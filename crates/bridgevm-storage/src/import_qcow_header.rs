//! Reject unsupported owned qcow2 dependencies before invoking image helpers.

use crate::import_paths::invalid;
use crate::StorageError;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub(crate) fn validate(path: &Path) -> Result<(), StorageError> {
    let mut header = [0u8; 104];
    File::open(path)?.read_exact(&mut header)?;
    let version = u32::from_be_bytes(header[4..8].try_into().unwrap());
    if &header[..4] != b"QFI\xfb" || !matches!(version, 2 | 3) {
        return Err(invalid("unsupported or malformed imported qcow2 header"));
    }
    // QEMU qcow2 format: crypt_method32..35; v3 incompatible_features72..79.
    // These must be checked without opening the named external dependencies.
    if header[32..36] != [0, 0, 0, 0] {
        return Err(invalid("encrypted qcow2 imports are unsupported"));
    }
    if version == 3 {
        let features = u64::from_be_bytes(header[72..80].try_into().unwrap());
        if features & 4 != 0 {
            return Err(invalid("external-data qcow2 imports are unsupported"));
        }
        if features & !0b11000 != 0 {
            return Err(invalid(
                "dirty, corrupt or unknown qcow2 features cannot be imported",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_external_corrupt_and_unknown_features_refuse_before_helper_use() {
        let path = crate::unique_temp_path("import-header-refusal");
        let mut header = [0u8; 104];
        header[..4].copy_from_slice(b"QFI\xfb");
        header[4..8].copy_from_slice(&3u32.to_be_bytes());
        for (encryption, feature) in [(1u32, 0u64), (2, 0), (0, 4), (0, 2), (0, 1), (0, 1 << 5)] {
            header[32..36].copy_from_slice(&encryption.to_be_bytes());
            header[72..80].copy_from_slice(&feature.to_be_bytes());
            std::fs::write(&path, header).unwrap();
            assert!(validate(&path).is_err());
        }
        std::fs::remove_file(path).unwrap();
    }
}
