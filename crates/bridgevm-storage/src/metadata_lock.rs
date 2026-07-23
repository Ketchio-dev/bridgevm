//! The advisory lock file with RAII release.

use crate::*;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

pub(crate) struct MetadataLock {
    pub(crate) path: PathBuf,
}

impl MetadataLock {
    pub(crate) fn acquire(bundle: &Path, name: &str) -> Result<Self, StorageError> {
        let path = bundle.join("metadata").join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        for _ in 0..100 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(StorageError::Io(error)),
            }
        }
        Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out waiting for metadata lock {}", path.display()),
        )))
    }
}

impl Drop for MetadataLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
