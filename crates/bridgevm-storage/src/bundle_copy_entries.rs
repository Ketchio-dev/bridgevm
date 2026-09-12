use crate::bundle_directory_permissions as permissions;
use crate::*;
use std::fs;
use std::path::Path;

pub(crate) fn copy_dir_all_inner(
    root: &Path,
    from: &Path,
    to: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), StorageError> {
    let mut entries = fs::read_dir(from)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from_path = entry.path();
        if should_skip_bundle_copy_path(&from_path) {
            continue;
        }
        let to_path = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            permissions::create(&to_path)?;
            copy_dir_all_inner(root, &from_path, &to_path, copied_files)?;
            permissions::copy_mode(&from_path, &to_path)?;
        } else if file_type.is_file() {
            fs::copy(&from_path, &to_path)?;
            let relative = from_path
                .strip_prefix(root)
                .map_err(|_| StorageError::UnsupportedBundleEntry(from_path.clone()))?;
            copied_files.push(relative.to_string_lossy().replace('\\', "/"));
        } else {
            return Err(StorageError::UnsupportedBundleEntry(from_path));
        }
    }
    Ok(())
}
