//! Validate the native library path without opening or modifying it.

use std::path::PathBuf;

pub(super) fn absolute_library(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("--library requires an absolute path without '..'".into());
    }
    Ok(path)
}
