//! Size-bounded JSON reads and atomic pretty JSON writes.

use crate::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, StorageError> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json_required(path)?))
}

pub(crate) const MAX_METADATA_JSON_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_json_required<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    let file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_METADATA_JSON_BYTES {
        return Err(StorageError::MetadataTooLarge {
            path: path.to_path_buf(),
            actual: size,
            maximum: MAX_METADATA_JSON_BYTES,
        });
    }
    let capacity = usize::try_from(size).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "metadata size exceeds host address space",
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_METADATA_JSON_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_JSON_BYTES {
        return Err(StorageError::MetadataTooLarge {
            path: path.to_path_buf(),
            actual: bytes.len() as u64,
            maximum: MAX_METADATA_JSON_BYTES,
        });
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) fn write_json_pretty_atomic<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> Result<(), StorageError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("metadata"),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&tmp, serde_json::to_string_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}
