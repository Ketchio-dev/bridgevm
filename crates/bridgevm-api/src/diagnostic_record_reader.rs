//! Race-free bounded reads for allowlisted diagnostic records.

use crate::*;
use std::os::unix::fs::OpenOptionsExt;

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "diagnostic allowlist input cannot be safely opened".to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("diagnostic allowlist input is not a regular file".to_string());
    }
    if metadata.len() > MAX_DIAGNOSTIC_FILE_BYTES {
        return Err("diagnostic allowlist input exceeds the per-file limit".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
