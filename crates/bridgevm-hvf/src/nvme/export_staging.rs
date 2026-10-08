//! An export may write and remove only the temporary inode it created.

#[cfg(test)]
#[path = "export_staging_tests.rs"]
mod tests;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct ExportStaging {
    pub(super) path: PathBuf,
    pub(super) file: File,
}

impl ExportStaging {
    pub(super) fn create(parent: &Path, destination: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".bridgevm-nvme-export-{}-{sequence}.tmp",
                std::process::id()
            ));
            if destination.file_name().is_some_and(|name| {
                path.file_name()
                    .unwrap()
                    .as_bytes()
                    .eq_ignore_ascii_case(name.as_bytes())
            }) {
                continue; // Also exclude case aliases on the default macOS filesystem.
            }
            // Names need not be secret: exclusive creation is the ownership boundary.
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free NVMe export staging name",
        ))
    }
}

impl Drop for ExportStaging {
    fn drop(&mut self) {
        // A renamed file has no remaining staging entry; never remove a replacement.
        if let (Ok(path), Ok(file)) = (fs::symlink_metadata(&self.path), self.file.metadata()) {
            if (path.dev(), path.ino()) == (file.dev(), file.ino()) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
