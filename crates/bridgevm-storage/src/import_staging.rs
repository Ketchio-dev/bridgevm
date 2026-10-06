//! Private staging permissions and failure cleanup never touch source directories.

use crate::{unique_temp_path, StorageError};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(crate) struct Staging {
    pub(crate) path: PathBuf,
    pub(crate) bundle: PathBuf,
}

impl Staging {
    pub(crate) fn create(parent: &Path) -> Result<Self, StorageError> {
        let path = parent.join(unique_temp_path(".bridgevm-import").file_name().unwrap());
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&path)?;
        Ok(Self {
            bundle: path.join("bundle"),
            path,
        })
    }

    pub(crate) fn writable(&self) -> Result<Vec<(PathBuf, fs::Permissions)>, StorageError> {
        let mut modes = Vec::new();
        make_writable(&self.path, &mut modes)?;
        Ok(modes)
    }

    pub(crate) fn restore(modes: Vec<(PathBuf, fs::Permissions)>) -> Result<(), StorageError> {
        for (path, mode) in modes.into_iter().rev() {
            fs::set_permissions(path, mode)?;
        }
        Ok(())
    }
}

fn make_writable(
    directory: &Path,
    modes: &mut Vec<(PathBuf, fs::Permissions)>,
) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.file_type().is_dir() {
        return Ok(());
    }
    modes.push((directory.to_path_buf(), metadata.permissions()));
    #[cfg(unix)]
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            make_writable(&entry.path(), modes)?;
        }
    }
    Ok(())
}

impl Drop for Staging {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = make_writable(&self.path, &mut Vec::new());
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
